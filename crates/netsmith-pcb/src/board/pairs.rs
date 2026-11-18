//! Which nets of a board are diff pairs: two nets whose names differ only in a suffix pair
//! (`netsmith_geom::PAIR_SUFFIXES`), the two nets of a `PairImpedance` (issue #1025), and two
//! nets the design states are a pair with no impedance, a `DiffPair` or the lines of a std bus
//! such as `Can` (issue #1193). The router routes every one side by side and matches its
//! lengths ([`Board::diff_pairs`]).

use super::*;

impl Board {
    /// The diff pairs of the board, each once, the positive net first: first the pairs its nets'
    /// names make, in the order of the first net's name (the first suffix pair that matches),
    /// then the pairs its design states, those of its impedance targets and then the others, in
    /// the order of their paths. A pair the design states takes its nets from a pair of names
    /// that has only one of them; a stated pair that the names already make keeps the names'
    /// place and order.
    pub fn diff_pairs(&self) -> Vec<(String, String)> {
        diff_pairs_of(&self.nets, &self.impedance_targets, &self.pairs)
    }

    /// For each net of the board, by its index in [`Board::nets`], the index of the other net of
    /// its diff pair ([`Board::diff_pairs`]), if it has one.
    pub fn diff_partners(&self) -> Vec<Option<usize>> {
        let index: std::collections::HashMap<&str, usize> = self
            .nets
            .iter()
            .enumerate()
            .map(|(n, net)| (net.name.as_str(), n))
            .collect();
        let mut partner = vec![None; self.nets.len()];
        for (p, n) in self.diff_pairs() {
            if let (Some(&a), Some(&b)) = (index.get(p.as_str()), index.get(n.as_str())) {
                partner[a] = Some(b);
                partner[b] = Some(a);
            }
        }
        partner
    }
}

/// The diff pairs of a board with `nets`, the impedance targets `targets` and the pairs its
/// design states with no impedance `stated`, as [`Board::diff_pairs`] gives them: for the placer,
/// which has no board yet.
pub fn diff_pairs_of(
    nets: &[BoardNet],
    targets: &[netsmith_core::ImpedanceTarget],
    stated: &[netsmith_core::NetPair],
) -> Vec<(String, String)> {
    let mut names: Vec<&str> = nets.iter().map(|n| n.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    let mut by_name: Vec<(String, String)> = Vec::new();
    for &p in &names {
        for (sp, sn) in netsmith_geom::PAIR_SUFFIXES {
            let Some(stem) = p.strip_suffix(sp) else {
                continue;
            };
            let n = format!("{stem}{sn}");
            if names.binary_search(&n.as_str()).is_ok() {
                by_name.push((p.to_string(), n));
                break;
            }
        }
    }
    let same =
        |a: &(String, String), p: &str, n: &str| (a.0 == p && a.1 == n) || (a.0 == n && a.1 == p);
    let touches = |a: &(String, String), net: &str| a.0 == net || a.1 == net;
    let mut chosen: Vec<(String, String)> = Vec::new();
    let impedance = targets
        .iter()
        .filter(|t| t.spacing.is_some() && t.nets.len() == 2)
        .map(|t| (t.nets[0].as_str(), t.nets[1].as_str()));
    let plain = stated.iter().map(|q| (q.p.as_str(), q.n.as_str()));
    for (p, n) in impedance.chain(plain) {
        if p == n || chosen.iter().any(|s| touches(s, p) || touches(s, n)) {
            continue;
        }
        chosen.push((p.to_string(), n.to_string()));
    }
    let mut out: Vec<(String, String)> = Vec::new();
    for pair in by_name {
        let replaced = chosen
            .iter()
            .any(|s| (touches(s, &pair.0) || touches(s, &pair.1)) && !same(s, &pair.0, &pair.1));
        if !replaced
            && !out
                .iter()
                .any(|o| touches(o, &pair.0) || touches(o, &pair.1))
        {
            out.push(pair);
        }
    }
    for s in chosen {
        if !out.iter().any(|o| same(o, &s.0, &s.1)) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_core::{ImpedanceTarget, NetPair};

    fn nets(names: &[&str]) -> Vec<BoardNet> {
        names
            .iter()
            .map(|n| BoardNet {
                name: n.to_string(),
                pads: Vec::new(),
            })
            .collect()
    }

    fn pair(p: &str, n: &str) -> (String, String) {
        (p.to_string(), n.to_string())
    }

    fn stated(path: &str, p: &str, n: &str) -> NetPair {
        NetPair {
            path: path.into(),
            p: p.into(),
            n: n.into(),
        }
    }

    #[test]
    fn the_names_make_pairs_first_and_the_design_states_the_rest() {
        let nets = nets(&["usb.dm", "usb.dp", "CANH", "CANL", "A", "B"]);
        assert_eq!(diff_pairs_of(&nets, &[], &[]), [pair("usb.dp", "usb.dm")]);
        // A stated pair adds its nets after the names' pairs; one the names make already keeps
        // the names' order.
        let found = diff_pairs_of(
            &nets,
            &[],
            &[
                stated("bus", "CANH", "CANL"),
                stated("usb", "usb.dm", "usb.dp"),
            ],
        );
        assert_eq!(found, [pair("usb.dp", "usb.dm"), pair("CANH", "CANL")]);
    }

    #[test]
    fn a_pair_the_design_states_takes_a_net_from_the_names_and_an_impedance_target_comes_first() {
        let nets = nets(&["X_P", "X_N", "A", "B"]);
        let target = ImpedanceTarget {
            path: "z".into(),
            nets: vec!["X_P".into(), "A".into()],
            ohms: 90.0,
            spacing: Some(0.15),
        };
        // `X_P` goes with `A`, as the target says, and the stated pair that names `A` too is
        // left out; `X_N` has no partner.
        let found = diff_pairs_of(&nets, &[target], &[stated("p", "A", "B")]);
        assert_eq!(found, [pair("X_P", "A")]);
    }
}
