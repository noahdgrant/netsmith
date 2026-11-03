//! Deciding whether two netlists are the same circuit.
//!
//! Both netlists are laid out as one bipartite graph of components and nets, joined by
//! pins. A component starts with the colour of its class (value and footprint) and a net
//! with a single colour. Refinement then repeatedly gives each node a new colour from its
//! old colour and the colours of what it touches, each edge labelled with its pin number,
//! until the colouring stops changing. If some colour has a different number of nodes on
//! the two sides, the netlists differ. If every colour has one node on each side, the
//! colours are the correspondence. Where parts are interchangeable (two equal resistors in
//! parallel, the rows of an LED array) a colour has several nodes on each side; one node
//! of A is then given a colour of its own together with each candidate of B in turn and
//! the colouring is refined again, which is individualization, up to a bound on the
//! refinements.
//!
//! When the netlists differ, a second pass finds what differs: it matches what the first
//! pass could still tell apart and spreads that match over the nets, so a single
//! mistake shows as the nets and pins around it and not as every component being unlike
//! its counterpart.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::model::{Component, Netlist};
use crate::value::normalize_value;

/// What makes components of the two netlists correspond.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Identity {
    /// Only their value, footprint and wiring.
    #[default]
    Free,
    /// Components with the same designator correspond, and no others do.
    ByRefdes,
    /// These `(designator in A, designator in B)` pairs correspond; the rest are matched by
    /// value, footprint and wiring.
    Map(Vec<(String, String)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub identity: Identity,
    /// Leave out the nets of a single pin on both sides.
    pub ignore_unconnected: bool,
    /// Compare values as numbers where they read as numbers (`4k7` is `4.7k`).
    pub loose_values: bool,
    /// How many refinements the search for an assignment of interchangeable parts may use
    /// before it gives up.
    pub max_refinements: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            identity: Identity::Free,
            ignore_unconnected: false,
            loose_values: false,
            max_refinements: 20_000,
        }
    }
}

/// Which netlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Side::A => "A",
            Side::B => "B",
        })
    }
}

/// A pin as one netlist names it: `R1.2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub refdes: String,
    pub pin: String,
}

impl std::fmt::Display for Pin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.refdes, self.pin)
    }
}

/// How the two netlists correspond, when they are the same circuit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correspondence {
    /// `(designator in A, designator in B)`, in A's order.
    pub components: Vec<(String, String)>,
    /// `(net name in A, net name in B)`, in A's order.
    pub nets: Vec<(String, String)>,
    /// How many times a choice was made between interchangeable parts. Those parts could be
    /// paired the other way round just as well.
    pub interchangeable: usize,
}

/// One way the netlists differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// A component with no counterpart, with its pins and the nets they are on.
    ComponentOnly {
        side: Side,
        refdes: String,
        value: String,
        footprint: String,
        pins: Vec<(String, String)>,
    },
    /// Counterparts whose value or footprint differ.
    ComponentClass {
        a: (String, String, String),
        b: (String, String, String),
    },
    /// Nets that correspond but do not have the same pins. Each pin is in the designators of
    /// its own netlist, with where its counterpart is in the other.
    NetPins {
        a: String,
        b: String,
        only_a: Vec<(Pin, String)>,
        only_b: Vec<(Pin, String)>,
    },
    /// A net of one netlist whose pins on corresponding components are on no net of the
    /// other that corresponds.
    NetOnly {
        side: Side,
        name: String,
        pins: Vec<Pin>,
    },
    /// The netlists differ but the difference could not be traced to a place.
    Unlocated(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Match(Correspondence),
    Different(Vec<Difference>),
    /// The search for an assignment of interchangeable parts passed its bound. Says where.
    Undecided(String),
}

/// What a component is compared by: its value and footprint, without the footprint's
/// library.
fn class(c: &Component, loose: bool) -> (String, String) {
    let footprint = c.footprint.rsplit(':').next().unwrap_or(&c.footprint);
    (normalize_value(&c.value, loose), footprint.to_string())
}

#[derive(Hash, PartialEq, Eq)]
enum Init {
    Class(String, String),
    Pair(usize),
    Alone(usize),
}

/// Both netlists as one graph. Components and nets are numbered across both, A first.
struct Joint<'n> {
    nl: [&'n Netlist; 2],
    /// Number of components and nets of A.
    ca: usize,
    na: usize,
    /// `(pin id, net)` for each component.
    comp_pins: Vec<Vec<(u32, usize)>>,
    /// `(component, pin id)` for each net.
    net_pins: Vec<Vec<(usize, u32)>>,
    /// Starting colour of each component.
    init: Vec<u32>,
    /// Pairs fixed by the identity, as global component numbers.
    fixed: Vec<(usize, usize)>,
    /// Whether a component may be paired by wiring: not when it is fixed, or when the
    /// identity says it has no counterpart.
    free: Vec<bool>,
}

impl<'n> Joint<'n> {
    fn new(a: &'n Netlist, b: &'n Netlist, options: &Options) -> Result<Joint<'n>, String> {
        let (ca, na) = (a.components.len(), a.nets.len());
        let mut pin_ids: HashMap<&str, u32> = HashMap::new();
        let mut comp_pins = Vec::new();
        let mut net_pins = Vec::new();
        for (side, nl) in [a, b].into_iter().enumerate() {
            let (c_off, n_off) = if side == 0 { (0, 0) } else { (ca, na) };
            for c in &nl.components {
                comp_pins.push(
                    c.pins
                        .iter()
                        .map(|(p, n)| {
                            let next = pin_ids.len() as u32;
                            (*pin_ids.entry(p).or_insert(next), n + n_off)
                        })
                        .collect(),
                );
            }
            for n in &nl.nets {
                net_pins.push(
                    n.pins
                        .iter()
                        .map(|(c, p)| {
                            let next = pin_ids.len() as u32;
                            (c + c_off, *pin_ids.entry(p).or_insert(next))
                        })
                        .collect(),
                );
            }
        }

        let total = ca + b.components.len();
        let mut fixed = Vec::new();
        match &options.identity {
            Identity::Free => {}
            Identity::ByRefdes => {
                for (i, c) in a.components.iter().enumerate() {
                    if let Some(j) = b.find(&c.refdes) {
                        fixed.push((i, ca + j));
                    }
                }
            }
            Identity::Map(pairs) => {
                let mut used = [BTreeSet::new(), BTreeSet::new()];
                for (ra, rb) in pairs {
                    let ia = a.find(ra).ok_or_else(|| {
                        format!("the map names {ra}, which is not a component of A")
                    })?;
                    let ib = b.find(rb).ok_or_else(|| {
                        format!("the map names {rb}, which is not a component of B")
                    })?;
                    if !used[0].insert(ia) {
                        return Err(format!("the map pairs {ra} twice"));
                    }
                    if !used[1].insert(ib) {
                        return Err(format!("the map pairs {rb} twice"));
                    }
                    fixed.push((ia, ca + ib));
                }
            }
        }
        let mut pair_of: HashMap<usize, usize> = HashMap::new();
        for (i, &(x, y)) in fixed.iter().enumerate() {
            pair_of.insert(x, i);
            pair_of.insert(y, i);
        }
        let mut interner: HashMap<Init, u32> = HashMap::new();
        let mut init = Vec::with_capacity(total);
        let mut free = Vec::with_capacity(total);
        for g in 0..total {
            let c = if g < ca {
                &a.components[g]
            } else {
                &b.components[g - ca]
            };
            let (key, is_free) = match pair_of.get(&g) {
                Some(&i) => (Init::Pair(i), false),
                None if options.identity == Identity::ByRefdes => (Init::Alone(g), false),
                None => {
                    let (v, f) = class(c, options.loose_values);
                    (Init::Class(v, f), true)
                }
            };
            let next = interner.len() as u32;
            init.push(*interner.entry(key).or_insert(next));
            free.push(is_free);
        }
        Ok(Joint {
            nl: [a, b],
            ca,
            na,
            comp_pins,
            net_pins,
            init,
            fixed,
            free,
        })
    }

    fn total_comps(&self) -> usize {
        self.comp_pins.len()
    }

    fn total_nets(&self) -> usize {
        self.net_pins.len()
    }

    fn side_of_comp(&self, g: usize) -> Side {
        if g < self.ca { Side::A } else { Side::B }
    }

    fn component(&self, g: usize) -> &Component {
        if g < self.ca {
            &self.nl[0].components[g]
        } else {
            &self.nl[1].components[g - self.ca]
        }
    }

    fn net_name(&self, g: usize) -> &str {
        if g < self.na {
            &self.nl[0].nets[g].name
        } else {
            &self.nl[1].nets[g - self.na].name
        }
    }

    /// Give every node a colour from its colour and its neighbours', until no colour splits.
    /// `each` sees the component colours at the start and after every round.
    fn refine(&self, comp: &mut Vec<u32>, net: &mut Vec<u32>, mut each: impl FnMut(&[u32])) {
        each(comp);
        let mut classes = distinct(comp) + distinct(net);
        loop {
            let mut comp_ids: HashMap<(u32, Vec<(u32, u32)>), u32> = HashMap::new();
            let mut net_ids: HashMap<(u32, Vec<(u32, u32)>), u32> = HashMap::new();
            let new_comp: Vec<u32> = (0..comp.len())
                .map(|g| {
                    let mut sig: Vec<(u32, u32)> = self.comp_pins[g]
                        .iter()
                        .map(|&(p, n)| (p, net[n]))
                        .collect();
                    sig.sort_unstable();
                    let next = comp_ids.len() as u32;
                    *comp_ids.entry((comp[g], sig)).or_insert(next)
                })
                .collect();
            let new_net: Vec<u32> = (0..net.len())
                .map(|g| {
                    let mut sig: Vec<(u32, u32)> = self.net_pins[g]
                        .iter()
                        .map(|&(c, p)| (p, comp[c]))
                        .collect();
                    sig.sort_unstable();
                    let next = net_ids.len() as u32;
                    *net_ids.entry((net[g], sig)).or_insert(next)
                })
                .collect();
            *comp = new_comp;
            *net = new_net;
            let now = distinct(comp) + distinct(net);
            if now == classes {
                return;
            }
            classes = now;
            each(comp);
        }
    }

    /// Whether every colour has as many components and nets in A as in B.
    fn balanced(&self, comp: &[u32], net: &[u32]) -> bool {
        let count = |colors: &[u32], split: usize| {
            let mut m: HashMap<u32, i64> = HashMap::new();
            for (g, c) in colors.iter().enumerate() {
                *m.entry(*c).or_default() += if g < split { 1 } else { -1 };
            }
            m.values().all(|&n| n == 0)
        };
        count(comp, self.ca) && count(net, self.na)
    }

    /// Components grouped by colour: `(in A, in B)`.
    fn cells(&self, comp: &[u32]) -> BTreeMap<u32, (Vec<usize>, Vec<usize>)> {
        let mut cells: BTreeMap<u32, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
        for (g, &c) in comp.iter().enumerate() {
            let cell = cells.entry(c).or_default();
            if g < self.ca {
                cell.0.push(g);
            } else {
                cell.1.push(g);
            }
        }
        cells
    }

    /// Search for a pairing of the components that is an isomorphism, individualizing where
    /// refinement leaves a choice.
    fn search(
        &self,
        mut comp: Vec<u32>,
        mut net: Vec<u32>,
        budget: &mut Budget,
    ) -> Result<Option<Pairing>, String> {
        budget.used += 1;
        self.refine(&mut comp, &mut net, |_| {});
        if !self.balanced(&comp, &net) {
            return Ok(None);
        }
        let cells = self.cells(&comp);
        let choice = cells
            .values()
            .filter(|(a, _)| a.len() > 1)
            .min_by_key(|(a, _)| a.len());
        let Some((cell_a, cell_b)) = choice else {
            return Ok(self.read_pairing(&cells));
        };
        if budget.used > budget.max {
            let class = self.component(cell_a[0]);
            let names = |g: &[usize]| {
                g.iter()
                    .map(|&g| self.component(g).refdes.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Err(format!(
                "could not decide whether the netlists match: {} components of {} ({}) are interchangeable by connectivity (A: {}; B: {}), and trying the ways to pair them passed the limit of {} refinements",
                cell_a.len(),
                if class.value.is_empty() {
                    "no value"
                } else {
                    &class.value
                },
                class.footprint,
                names(cell_a),
                names(cell_b),
                budget.max
            ));
        }
        let fresh = comp.iter().max().map_or(0, |m| m + 1);
        let x = cell_a[0];
        for &y in cell_b {
            let mut trial = comp.clone();
            trial[x] = fresh;
            trial[y] = fresh;
            if let Some(mut pairing) = self.search(trial, net.clone(), budget)? {
                pairing.interchangeable += 1;
                return Ok(Some(pairing));
            }
        }
        Ok(None)
    }

    /// The pairing a discrete colouring gives, if it is an isomorphism.
    fn read_pairing(&self, cells: &BTreeMap<u32, (Vec<usize>, Vec<usize>)>) -> Option<Pairing> {
        let mut partner = vec![None; self.total_comps()];
        for (a, b) in cells.values() {
            if let ([x], [y]) = (a.as_slice(), b.as_slice()) {
                partner[*x] = Some(*y);
                partner[*y] = Some(*x);
            }
        }
        let nets = self.net_correspondence(&partner);
        let wired = (0..self.ca).all(|a| {
            partner[a].is_some_and(|b| {
                let (pa, pb) = (&self.comp_pins[a], &self.comp_pins[b]);
                pa.len() == pb.len()
                    && pa
                        .iter()
                        .zip(pb)
                        .all(|(x, y)| x.0 == y.0 && nets[x.1] == Some(y.1))
            })
        });
        let all_nets = (0..self.na).all(|n| nets[n].is_some());
        (wired && all_nets).then_some(Pairing {
            partner,
            nets,
            interchangeable: 0,
        })
    }

    /// Which net corresponds to which, from the components that are paired: a net of A
    /// corresponds to the net of B that holds the counterparts of its pins, most pins first.
    /// Indexed by global net number, both ways.
    fn net_correspondence(&self, partner: &[Option<usize>]) -> Vec<Option<usize>> {
        let mut votes: HashMap<(usize, usize), usize> = HashMap::new();
        for (a, &b) in partner.iter().enumerate().take(self.ca) {
            let Some(b) = b else { continue };
            for &(pin, na) in &self.comp_pins[a] {
                if let Some(&(_, nb)) = self.comp_pins[b].iter().find(|(p, _)| *p == pin) {
                    *votes.entry((na, nb)).or_default() += 1;
                }
            }
        }
        let mut ranked: Vec<_> = votes.into_iter().collect();
        ranked.sort_by_key(|&((a, b), n)| (std::cmp::Reverse(n), a, b));
        let mut nets = vec![None; self.total_nets()];
        for ((a, b), _) in ranked {
            if nets[a].is_none() && nets[b].is_none() {
                nets[a] = Some(b);
                nets[b] = Some(a);
            }
        }
        nets
    }

    /// The net of a pin of a component, by pin number.
    fn net_at(&self, g: usize, pin: u32) -> Option<usize> {
        self.comp_pins[g]
            .iter()
            .find(|(p, _)| *p == pin)
            .map(|(_, n)| *n)
    }
}

fn distinct(colors: &[u32]) -> usize {
    colors.iter().collect::<BTreeSet<_>>().len()
}

struct Budget {
    used: usize,
    max: usize,
}

struct Pairing {
    /// Global component to global component, both ways.
    partner: Vec<Option<usize>>,
    nets: Vec<Option<usize>>,
    interchangeable: usize,
}

/// Decide whether two netlists are the same circuit, and if they are not, find what
/// differs. `Err` is a problem with the options (a map naming a component that is not
/// there).
pub fn compare(a: &Netlist, b: &Netlist, options: &Options) -> Result<Outcome, String> {
    let (a, b) = if options.ignore_unconnected {
        (a.without_single_pin_nets(), b.without_single_pin_nets())
    } else {
        (a.clone(), b.clone())
    };
    let joint = Joint::new(&a, &b, options)?;
    let mut budget = Budget {
        used: 0,
        max: options.max_refinements,
    };
    let comp = joint.init.clone();
    let net = vec![0; joint.total_nets()];
    match joint.search(comp, net, &mut budget) {
        Err(message) => Ok(Outcome::Undecided(message)),
        Ok(Some(pairing)) => {
            let mut differences = class_differences(&joint, &pairing.partner, options);
            if differences.is_empty() {
                Ok(Outcome::Match(joint.correspondence(&pairing)))
            } else {
                sort_differences(&mut differences);
                Ok(Outcome::Different(differences))
            }
        }
        Ok(None) => {
            let mut differences = joint.locate(options);
            if differences.is_empty() {
                differences.push(Difference::Unlocated(
                    "the two netlists are not the same circuit, but no component or net can be pointed to"
                        .into(),
                ));
            }
            sort_differences(&mut differences);
            Ok(Outcome::Different(differences))
        }
    }
}

/// Counterparts of a fixed pairing whose value or footprint differ.
fn class_differences(
    joint: &Joint,
    partner: &[Option<usize>],
    options: &Options,
) -> Vec<Difference> {
    let mut out = Vec::new();
    for (a, &b) in partner.iter().enumerate().take(joint.ca) {
        let Some(b) = b else { continue };
        let (ca, cb) = (joint.component(a), joint.component(b));
        if class(ca, options.loose_values) != class(cb, options.loose_values) {
            out.push(Difference::ComponentClass {
                a: (ca.refdes.clone(), ca.value.clone(), ca.footprint.clone()),
                b: (cb.refdes.clone(), cb.value.clone(), cb.footprint.clone()),
            });
        }
    }
    out
}

fn sort_differences(d: &mut [Difference]) {
    let rank = |d: &Difference| match d {
        Difference::ComponentClass { a, .. } => (0, a.0.clone()),
        Difference::ComponentOnly { side, refdes, .. } => (1 + *side as usize, refdes.clone()),
        Difference::NetPins { a, .. } => (3, a.clone()),
        Difference::NetOnly { side, name, .. } => (4 + *side as usize, name.clone()),
        Difference::Unlocated(m) => (6, m.clone()),
    };
    d.sort_by_key(rank);
}

impl Joint<'_> {
    fn correspondence(&self, p: &Pairing) -> Correspondence {
        let mut components = Vec::new();
        for a in 0..self.ca {
            if let Some(b) = p.partner[a] {
                components.push((
                    self.component(a).refdes.clone(),
                    self.component(b).refdes.clone(),
                ));
            }
        }
        let mut nets = Vec::new();
        for a in 0..self.na {
            if let Some(b) = p.nets[a] {
                nets.push((self.net_name(a).to_string(), self.net_name(b).to_string()));
            }
        }
        Correspondence {
            components,
            nets,
            interchangeable: p.interchangeable,
        }
    }

    /// Find where two netlists that are not the same circuit differ, by pairing components
    /// as far as the evidence goes and describing what is left over.
    ///
    /// Each round of the loop gives the paired components colours of their own and
    /// refines again. Then, in order, it pairs what refinement can tell apart (a component
    /// that is alone of its colour on each side, in any round: early rounds see only a
    /// neighbourhood, so a mistake far away does not hide it), then what the nets already
    /// paired imply, and last, where nothing is certain, one of a group of components that
    /// look alike with one of the other side's, preferring equal designators.
    fn locate(&self, options: &Options) -> Vec<Difference> {
        let mut partner: Vec<Option<usize>> = vec![None; self.total_comps()];
        for &(x, y) in &self.fixed {
            partner[x] = Some(y);
            partner[y] = Some(x);
        }
        let base = self.init.iter().max().map_or(0, |m| m + 1);
        loop {
            let mut comp = self.init.clone();
            for (g, p) in partner.iter().enumerate() {
                if let Some(p) = p {
                    comp[g] = base + g.min(*p) as u32;
                }
            }
            let mut net = vec![0; self.total_nets()];
            let mut history: Vec<Vec<u32>> = Vec::new();
            self.refine(&mut comp, &mut net, |colors| history.push(colors.to_vec()));

            let mut progress = false;
            for colors in &history {
                for (a, b) in self.cells(colors).values() {
                    let alone = |g: &[usize]| match g {
                        [x] if partner[*x].is_none() && self.free[*x] => Some(*x),
                        _ => None,
                    };
                    if let (Some(x), Some(y)) = (alone(a), alone(b)) {
                        partner[x] = Some(y);
                        partner[y] = Some(x);
                        progress = true;
                    }
                }
            }
            if progress || self.spread(&mut partner, options) {
                continue;
            }
            let guess = history.iter().rev().find_map(|colors| {
                self.cells(colors)
                    .into_values()
                    .map(|(a, b)| {
                        let open = |g: Vec<usize>| -> Vec<usize> {
                            g.into_iter()
                                .filter(|&g| partner[g].is_none() && self.free[g])
                                .collect()
                        };
                        (open(a), open(b))
                    })
                    .filter(|(a, b)| a.len() == b.len() && a.len() > 1)
                    .min_by_key(|(a, _)| a.len())
            });
            let Some((cell_a, cell_b)) = guess else { break };
            let same_designator = cell_a.iter().find_map(|&x| {
                cell_b
                    .iter()
                    .find(|&&y| self.component(x).refdes == self.component(y).refdes)
                    .map(|&y| (x, y))
            });
            let (x, y) = same_designator.unwrap_or((cell_a[0], cell_b[0]));
            partner[x] = Some(y);
            partner[y] = Some(x);
        }
        self.describe(&partner, options)
    }

    /// Pair unpaired components with the unpaired component of the other side whose pins are
    /// on the most nets that correspond. Whether any were paired.
    fn spread(&self, partner: &mut [Option<usize>], options: &Options) -> bool {
        let nets = self.net_correspondence(partner);
        let wired = |g: usize| self.comp_pins[g].len();
        let mut candidates = Vec::new();
        for a in (0..self.ca).filter(|&a| partner[a].is_none() && self.free[a]) {
            let mut scores: BTreeMap<usize, usize> = BTreeMap::new();
            for &(pin, na) in &self.comp_pins[a] {
                let Some(nb) = nets[na] else { continue };
                for &(c, p) in &self.net_pins[nb] {
                    if p == pin && c >= self.ca && partner[c].is_none() && self.free[c] {
                        *scores.entry(c).or_default() += 1;
                    }
                }
            }
            for (b, score) in scores {
                let same = class(self.component(a), options.loose_values)
                    == class(self.component(b), options.loose_values);
                if same || score * 2 > wired(a).max(wired(b)) {
                    let name = self.component(a).refdes == self.component(b).refdes;
                    candidates.push((
                        std::cmp::Reverse(score),
                        std::cmp::Reverse(same),
                        std::cmp::Reverse(name),
                        a,
                        b,
                    ));
                }
            }
        }
        candidates.sort();
        let mut any = false;
        for (_, _, _, a, b) in candidates {
            if partner[a].is_none() && partner[b].is_none() {
                partner[a] = Some(b);
                partner[b] = Some(a);
                any = true;
            }
        }
        any
    }

    /// The differences between two netlists whose components are paired as `partner` says.
    fn describe(&self, partner: &[Option<usize>], options: &Options) -> Vec<Difference> {
        let mut out = class_differences(self, partner, options);
        let nets = self.net_correspondence(partner);
        let pin_name = |g: usize, pin: u32| -> String {
            let (c, p) = (self.component(g), self.pin_text(g, pin));
            format!("{}.{p}", c.refdes)
        };

        for (g, p) in partner.iter().enumerate() {
            if p.is_none() {
                let c = self.component(g);
                let pins = self.comp_pins[g]
                    .iter()
                    .map(|&(p, n)| (self.pin_text(g, p), self.net_name(n).to_string()))
                    .collect();
                out.push(Difference::ComponentOnly {
                    side: self.side_of_comp(g),
                    refdes: c.refdes.clone(),
                    value: c.value.clone(),
                    footprint: c.footprint.clone(),
                    pins,
                });
            }
        }

        // Where a pin of one side is in the other.
        let elsewhere = |g: usize, pin: u32, other: Side| -> String {
            let Some(h) = partner[g] else {
                return String::new();
            };
            match self.net_at(h, pin) {
                Some(n) => format!(
                    "in {other}, {} is on net `{}`",
                    pin_name(h, pin),
                    self.net_name(n)
                ),
                None => format!("in {other}, {} is not connected", pin_name(h, pin)),
            }
        };

        for (na, &nb) in nets.iter().enumerate().take(self.na) {
            let Some(nb) = nb else { continue };
            let mut only_a = Vec::new();
            for &(c, pin) in &self.net_pins[na] {
                let Some(h) = partner[c] else { continue };
                if self.net_at(h, pin) != Some(nb) {
                    only_a.push((self.pin_of(c, pin), elsewhere(c, pin, Side::B)));
                }
            }
            let mut only_b = Vec::new();
            for &(c, pin) in &self.net_pins[nb] {
                let Some(h) = partner[c] else { continue };
                if self.net_at(h, pin) != Some(na) {
                    only_b.push((self.pin_of(c, pin), elsewhere(c, pin, Side::A)));
                }
            }
            if !only_a.is_empty() || !only_b.is_empty() {
                out.push(Difference::NetPins {
                    a: self.net_name(na).to_string(),
                    b: self.net_name(nb).to_string(),
                    only_a,
                    only_b,
                });
            }
        }

        // A net with no counterpart that still touches components that do have one.
        for (g, counterpart) in nets.iter().enumerate() {
            if counterpart.is_some() {
                continue;
            }
            let pins: Vec<Pin> = self.net_pins[g]
                .iter()
                .filter(|&&(c, _)| partner[c].is_some())
                .map(|&(c, p)| self.pin_of(c, p))
                .collect();
            if !pins.is_empty() {
                out.push(Difference::NetOnly {
                    side: if g < self.na { Side::A } else { Side::B },
                    name: self.net_name(g).to_string(),
                    pins,
                });
            }
        }
        out
    }

    fn pin_of(&self, c: usize, pin: u32) -> Pin {
        Pin {
            refdes: self.component(c).refdes.clone(),
            pin: self.pin_text(c, pin),
        }
    }

    /// The pin number a pin id stands for on a component.
    fn pin_text(&self, g: usize, pin: u32) -> String {
        let position = self.comp_pins[g].iter().position(|(p, _)| *p == pin);
        position
            .map(|i| self.component(g).pins[i].0.clone())
            .unwrap_or_default()
    }
}
