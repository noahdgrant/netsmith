//! The `track drop` guideline (issue #652): a power net whose routed tracks drop more than
//! [`DEFAULT_DROP_BUDGET`] of its rail's voltage at the net's worst-case current. `fab` runs it
//! once the board is routed, since the drop depends on the length the router draws; how the
//! drop is measured is in `netsmith_pcb`'s `track_drop.rs`.
//!
//! **A guideline, as the track width against current is (ADR 0036, ADR 0030).** It warns: a
//! `warning:` line from `fab` and a line in `fab-notes.txt`. With `guidelines = "error"` or
//! `--strict` it is an error and `fab` stops before it writes the package. The budget is fixed
//! for now (decided by the agent under the owner's delegation, issue #652): a way for a design
//! to state its own is a language choice left to the owner. A net whose rail has no known
//! voltage, or whose current is not known, is not checked.

use netsmith_pcb::{Board, DEFAULT_DROP_BUDGET, TrackDrop};

/// The rule's name, as a guideline's message names it: `Board: track drop guideline: ...`.
pub const RULE: &str = "track drop";

/// A finding for each net of the routed `board` whose tracks drop more than the budget of its
/// rail's voltage, from `voltages` (by net name; `netsmith_design::guidelines::net_voltages`).
pub fn findings(board: &Board, voltages: &[(String, f64)]) -> Vec<String> {
    board
        .track_drops()
        .iter()
        .filter_map(|d| {
            let &(_, volts) = voltages.iter().find(|(n, _)| *n == d.net)?;
            finding(d, volts, DEFAULT_DROP_BUDGET)
        })
        .collect()
}

/// The message for a net whose path drops more than `budget` of `volts`, or `None`.
pub fn finding(d: &TrackDrop, volts: f64, budget: f64) -> Option<String> {
    let allowed = volts * budget;
    if volts <= 0.0 || d.drop() <= allowed * (1.0 + 1e-9) {
        return None;
    }
    let fix = match d.width_for(allowed) {
        Some(w) => format!(
            "give a rail on the net `.min_track &= {}mm..` (that path at that width drops no more), or give the net an outer pour",
            number(w)
        ),
        None => "no width brings that path within it: give the net an outer pour or a plane, or put the parts nearer".to_string(),
    };
    Some(format!(
        "net `{}` drops up to {} V on its tracks at {} ({} of its {} V; the budget is {}): from {} to {} through {} mm of track, {} mm at its narrowest, in {} mm copper, with the whole current between the two pads with the most resistance between them; {fix}",
        d.net,
        sig(d.drop()),
        amps(d.current),
        percent(d.drop() / volts),
        sig(volts),
        percent(budget),
        d.from,
        d.to,
        number(d.length),
        number(d.narrowest),
        number(d.thickness),
    ))
}

/// A number with at most three decimals and no trailing zeros: `50`, `0.98`, `0.035`.
fn number(x: f64) -> String {
    let text = format!("{x:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A number to three significant digits, no trailing zeros: `0.0501`, `5`, `3.3`.
fn sig(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    let decimals = (2 - x.abs().log10().floor() as i32).max(0) as usize;
    let text = format!("{x:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

/// A current to three significant digits: `500 mA`, `2 A`.
fn amps(x: f64) -> String {
    if x < 1.0 {
        format!("{} mA", sig(x * 1e3))
    } else {
        format!("{} A", sig(x))
    }
}

/// A fraction as a percentage to two significant digits at most: `1%`, `1.3%`, `0.5%`.
fn percent(x: f64) -> String {
    let p = x * 100.0;
    let text = if p >= 10.0 {
        format!("{p:.0}")
    } else {
        format!("{p:.1}")
    };
    format!(
        "{}%",
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.')
        } else {
            &text
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_pcb::PadRef;

    fn drop_of(current: f64, length: f64, width: f64) -> TrackDrop {
        let thickness = 0.035;
        TrackDrop {
            net: "5V".into(),
            current,
            resistance: netsmith_pcb::COPPER_RESISTIVITY * length / (width * thickness),
            from: PadRef {
                refdes: "J1".into(),
                number: "1".into(),
            },
            to: PadRef {
                refdes: "J2".into(),
                number: "1".into(),
            },
            length,
            narrowest: width,
            runs: vec![(length, width)],
            thickness,
        }
    }

    #[test]
    fn a_net_over_the_budget_is_named_with_the_width_that_brings_it_within() {
        // 2 A through 100 mm of 0.98 mm in 1 oz: 0.0501 ohm, 0.1 V, 2% of 5 V.
        let d = drop_of(2.0, 100.0, 0.98);
        assert_eq!(
            finding(&d, 5.0, 0.01).unwrap(),
            "net `5V` drops up to 0.1 V on its tracks at 2 A (2% of its 5 V; the budget is 1%): from J1.1 to J2.1 through 100 mm of track, 0.98 mm at its narrowest, in 0.035 mm copper, with the whole current between the two pads with the most resistance between them; give a rail on the net `.min_track &= 1.97mm..` (that path at that width drops no more), or give the net an outer pour"
        );
    }

    #[test]
    fn a_net_within_the_budget_has_no_finding() {
        // 2 A through 50 mm of 0.98 mm: 0.05 V, 1% of 5 V and no more.
        let d = drop_of(2.0, 50.0, 0.98);
        assert!(d.drop() < 0.0502 && d.drop() > 0.05, "{}", d.drop());
        assert_eq!(finding(&d, 5.0, 0.0101), None);
        assert!(finding(&d, 5.0, 0.0099).is_some());
    }

    #[test]
    fn a_path_no_width_brings_within_says_so() {
        let d = drop_of(20.0, 5000.0, 0.2);
        assert!(
            finding(&d, 3.3, 0.01)
                .unwrap()
                .contains("no width brings that path within it")
        );
    }
}
