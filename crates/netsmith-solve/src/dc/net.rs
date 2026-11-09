//! The network of an island and its worst case: every corner of the ranges and every state of
//! the diodes, each solved as one operating point, and the range each quantity takes over all
//! of them (ADR 0076, section 3).
//!
//! Interval arithmetic counts a value once for each time it is written (`10 * R2 / (R1 + R2)`
//! with both resistors `1 kohm +/- 1 %` is `4.90 V..5.10 V` that way, `4.95 V..5.05 V` for the
//! circuit). A corner solve gives each value one number in each solve, so nothing is counted
//! twice. The range of a quantity is the hull of its values over the combinations. It is the
//! range of the circuit where the quantity is monotone in each value (a node voltage or a branch
//! current of a network with one source, a series chain); a ladder with two sources need not be,
//! and [`check_interior`] (for tests) draws points inside the ranges to say how many fall
//! outside the hull.
//!
//! The circuit is the nodes of one island: the nets that no source holds (`free`, the unknowns)
//! and the nets a source holds (`held`, each a range that is one of the island's values). The
//! elements are resistors, diodes, current sources, the inputs of regulators, MOSFETs as
//! switches and logic pins that drive. A diode blocks or holds its forward voltage and carries a
//! current of at least zero; a state that is not what the solution says (a diode conducting a
//! negative current, one blocking with more than its forward voltage across it) is refused and
//! not counted. A MOSFET is off or on through its channel resistance, and is refused when on with
//! the gate below the least threshold or off above the greatest; a gate that no conducting path
//! joins to a held node floats, and both states stay. A pin is high, low or high impedance, one
//! state in each combination.
//!
//! A regulator's input (ADR 0076, amendment 3) draws a current set by the regulator's output
//! and not by its own voltage: a linear one draws what its output supplies, a switching one
//! `Pout / (eff * Vin)`, which depends on the input voltage that its own current lowers through
//! the resistance in front of it. That is solved by a fixed point from zero, whose iterates rise
//! to the least fixed point when there is one; when there is none the input falls to nothing
//! and the combination is a *collapse*: the source cannot give what the load asks. A collapse
//! is reported when the states are the circuit's, and never as the side effect of a refused
//! state (a diode blocking in front of a regulator leaves it no input, and is no finding).

use netsmith_quantity::{Dimension, Quantity};

/// The smallest resistance that is a conductance of the matrix, 1 mohm (a conductance of 1000 S
/// is the largest entry the singularity test below is made for). A resistance under it, down to
/// zero (a bead's `0 ohm..70 mohm`), is solved as a branch with its own resistance, which holds
/// any value exactly (ADR 0076, amendment 16); the resistance is only raised to this where a
/// branch cannot be used (a resistor between two held nodes or from a node to itself, and a
/// combination whose branches make a singular matrix).
pub(crate) const MIN_RESISTANCE: f64 = 1e-3;

/// A conductance from every free node to the ground, as SPICE does, added only to a combination
/// whose matrix is singular without it, so that a node that nothing holds (a diode that blocks
/// and a load behind it) has a voltage and not a singular matrix. A picoamp per megavolt: it
/// moves the current of a 10 Mohm resistor by 1e-8 of itself, and a network that is solved
/// without it is not touched.
const GMIN: f64 = 1e-15;

/// The size of a pivot, relative to the largest entry of the matrix, below which it is zero: an
/// elimination of conductances up to 1000 S leaves a rounding of about 1e-13 S where a cluster
/// of nodes is held by nothing.
const SINGULAR: f64 = 1e-13;

/// A state that is wrong by no more than this is kept: a diode that conducts may carry `-1 pA`,
/// one that blocks may have 1 nV across it the wrong way.
const CURRENT_SLACK: f64 = 1e-12;
const VOLTAGE_SLACK: f64 = 1e-9;

/// The most rounds of the fixed point of the regulators' inputs, and the round after which it
/// takes half steps. The rise from zero settles in nine or fewer on a real power tree.
const MAX_ROUNDS: usize = 400;
const HALF_STEP_FROM: usize = 60;

/// An input below this at a regulator that draws a power is a collapse: the load asks for more
/// than the source can give through the resistance in between.
const COLLAPSE_VOLTS: f64 = 1e-3;

/// What the rounds of the fixed point cost, as a factor on the work of one solve, for the limit
/// of an island with a switching regulator in it.
const ROUNDS_WORK: u64 = 12;

/// More combinations than this is left alone: the island is too large to enumerate, and the
/// cheap bound of ADR 0026 is all it gets. A combination is a few microseconds in an optimised
/// build, so the limit is a tenth of a second at the most for one island.
pub(crate) const MAX_COMBINATIONS: u64 = 1 << 16;

/// The work an island may take, in the units of one multiplication of the elimination: a
/// combination costs about the cube of the network's size (the free nodes and the diodes that
/// conduct) and a constant. An island of fifteen nodes is 65536 combinations; one of thirty is
/// 8000, so that no island takes more than a tenth of a second in an optimised build.
pub(crate) const MAX_WORK: u64 = 1 << 28;

/// A closed range of reals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Range {
    pub lo: f64,
    pub hi: f64,
}

impl Range {
    pub const EMPTY: Range = Range {
        lo: f64::INFINITY,
        hi: f64::NEG_INFINITY,
    };

    pub fn new(a: f64, b: f64) -> Range {
        Range {
            lo: a.min(b),
            hi: a.max(b),
        }
    }

    pub fn point(v: f64) -> Range {
        Range { lo: v, hi: v }
    }

    pub fn is_point(self) -> bool {
        self.lo == self.hi
    }

    pub fn is_empty(self) -> bool {
        self.lo > self.hi
    }

    fn include(&mut self, v: f64) {
        self.lo = self.lo.min(v);
        self.hi = self.hi.max(v);
    }

    /// The values a corner enumeration tries: one for a point, the two ends otherwise.
    fn corners(self) -> Vec<f64> {
        if self.is_point() {
            vec![self.lo]
        } else {
            vec![self.lo, self.hi]
        }
    }
}

/// What a regulator's input draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Draw {
    /// What its output supplies: a current of the range `i`.
    Linear { i: Range },
    /// `p / (eff * v_in) + iq` for the output power `p`, the efficiency `eff` and the quiescent
    /// current `iq` (a point at zero for a regulator that states none).
    Switching { p: Range, eff: Range, iq: Range },
}

/// What an element is. Nodes are numbered `0..free` for the free nodes and then the held ones.
#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Resistor {
        a: usize,
        b: usize,
        r: Range,
    },
    /// Blocks, or holds `vf` from `anode` to `cathode` and carries a current of at least zero.
    Diode {
        anode: usize,
        cathode: usize,
        vf: Range,
    },
    /// A current `i` taken out of `from` and put into `to`.
    Current {
        from: usize,
        to: usize,
        i: Range,
    },
    /// The input of a regulator: a current taken out of `from` (its input's positive pin) and
    /// put into `to` (its ground), set by what its output asks.
    Regulator {
        from: usize,
        to: usize,
        draw: Draw,
    },
    /// A MOSFET as a switch: `ron` between `drain` and `source` when it is on, open when it is
    /// off. The gate draws nothing. `vth` is the range of the gate threshold, and the drive is
    /// `V(gate) - V(source)` for an N-channel part and `V(source) - V(gate)` for a P-channel
    /// one (the values of the part are magnitudes). On is refused when the drive is below the
    /// least threshold, off when it is above the greatest; between the two both are kept.
    Mosfet {
        gate: usize,
        drain: usize,
        source: usize,
        p_channel: bool,
        vth: Range,
        ron: Range,
    },
    /// A logic pin that drives (ADR 0076, amendment 4): high, its `node` joined through a
    /// resistance to the node of its supply (`high`); low, to the node of its ground (`low`);
    /// and, when the pin can let go of the line, high impedance (`z`), with no leakage (a
    /// pin does not state one).
    Pin {
        node: usize,
        high: Option<(usize, Range)>,
        low: Option<(usize, Range)>,
        z: bool,
        /// The leakage of the pin when it lets go (`Ii` of the datasheet, ADR 0076, amendment
        /// 8): the node of its supply, the node of its ground, and the resistance to either.
        /// High impedance is then an open circuit, or a leak to the supply, or one to the
        /// ground.
        leak: Option<(usize, usize, Range)>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Element {
    pub name: String,
    pub kind: Kind,
}

/// A node that a source holds: its voltage is a range that the combinations take the ends of.
#[derive(Clone, Debug)]
pub(crate) struct Held {
    pub name: String,
    pub v: Range,
}

/// One island: the nodes no source holds, the nodes a source holds, and what is between them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Island {
    pub free: usize,
    pub held: Vec<Held>,
    pub elements: Vec<Element>,
}

/// A choice of one element or of one held node, with the words for it.
#[derive(Clone, Debug)]
enum Point {
    R {
        a: usize,
        b: usize,
        r: f64,
    },
    I {
        from: usize,
        to: usize,
        i: f64,
    },
    /// A resistance under `MIN_RESISTANCE`, down to a short: a branch whose current is an
    /// unknown of the matrix and whose equation is `V(a) - V(b) = r * I`, exact for any `r`
    /// and with no large conductance in the matrix.
    Branch {
        a: usize,
        b: usize,
        r: f64,
    },
    DiodeOn {
        a: usize,
        k: usize,
        vf: f64,
    },
    DiodeOff {
        a: usize,
        k: usize,
        vf: f64,
    },
    /// A regulator's input that draws `p / (eff * v_in) + iq`, found by the fixed point.
    Sink {
        from: usize,
        to: usize,
        p: f64,
        eff: f64,
        iq: f64,
    },
    /// A switch that is off, a pin that lets go of its line: no current between `a` and `b`.
    /// A MOSFET's gate and threshold are kept to judge the state.
    Open {
        a: usize,
        b: usize,
        gate: Option<Gate>,
    },
    /// A MOSFET that is on: `r` between `d` and `s`.
    MosOn {
        d: usize,
        s: usize,
        r: f64,
        gate: Gate,
    },
}

/// What a MOSFET's state is judged by: its gate node, and the nodes and range that make the
/// drive.
#[derive(Clone, Copy, Debug)]
struct Gate {
    gate: usize,
    source: usize,
    p_channel: bool,
    vth: Range,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Error {
    /// The product of the variants is more than the limit.
    TooManyCombinations { combinations: u64, limit: u64 },
    /// No combination had consistent states and a solution.
    NoOperatingPoint,
    /// A matrix that cannot be solved (a loop of ideal sources, a node nothing connects).
    Singular,
}

/// A combination in which a regulator asks for more than the source can give: the input fell
/// to `vin` volts and no operating point exists.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Collapse {
    /// The element of the island (a regulator's input).
    pub element: usize,
    pub combination: u64,
    pub vin: f64,
}

/// What one operating point was.
enum Op {
    /// A solution whose states are the circuit's.
    Point,
    /// A state is not the solution's, or a regulator's input is the side effect of one.
    Refused,
    /// The states are the circuit's and a regulator's input fell to `vin` (the element's index).
    Collapse { element: usize, vin: f64 },
}

/// The result: a range per quantity, and the combination that made each end.
#[derive(Debug)]
pub(crate) struct Report {
    ranges: Vec<Range>,
    at_lo: Vec<u64>,
    at_hi: Vec<u64>,
    /// For each variable (held nodes first, then elements): its name and the label of each
    /// variant, to name a combination.
    variables: Vec<(String, Vec<String>)>,
    nodes: usize,
    elements: usize,
    held: usize,
    pub combinations: u64,
    pub consistent: u64,
    /// The first combination in which each regulator collapsed, by element.
    pub collapses: Vec<Collapse>,
}

/// Which quantity of an element or node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    NodeVoltage(usize),
    /// The current through an element, from `a` to `b` (a diode: anode to cathode).
    Current(usize),
    /// `V(a) - V(b)` across an element.
    Drop(usize),
    /// The current that a held node gives into the elements of the island (the index into
    /// `Island::held`): what a regulator's output supplies to the island.
    HeldDraw(usize),
    /// The current into a free node at one operating point: the sum of what the elements put
    /// into it, each at that point's own value, so that the loads that follow the supply are
    /// at their values for one corner and not each at its largest (ADR 0076, amendment 8).
    Through(usize),
}

impl Report {
    fn index(&self, q: Which) -> usize {
        match q {
            Which::NodeVoltage(n) => n,
            Which::Current(e) => self.nodes + e,
            Which::Drop(e) => self.nodes + self.elements + e,
            Which::HeldDraw(h) => self.nodes + 2 * self.elements + h,
            Which::Through(k) => self.nodes + 2 * self.elements + self.held + k,
        }
    }

    pub fn range(&self, q: Which) -> Range {
        self.ranges[self.index(q)]
    }

    /// The states of the combination that produced the low or the high end of a quantity: the
    /// variants of the values that have a choice, in the order the island lists them.
    #[cfg(test)]
    pub fn produced_by(&self, q: Which, high: bool) -> String {
        let i = self.index(q);
        let c = if high { self.at_hi[i] } else { self.at_lo[i] };
        self.describe(c)
    }

    /// The states that matter for an end of a quantity: those of the combination that made it,
    /// less each value that can be put at its other end (or in its other state) with the
    /// quantity at the same value. A current that is a load's is the load's, whatever the
    /// supply is, and the supply is not named.
    pub fn states_at(&self, island: &Island, q: Which, high: bool) -> String {
        let i = self.index(q);
        let combination = if high { self.at_hi[i] } else { self.at_lo[i] };
        let variants = island.variants();
        let mut picks: Vec<usize> = Vec::with_capacity(variants.len());
        let mut c = combination;
        for v in &variants {
            let n = v.len() as u64;
            picks.push((c % n) as usize);
            c /= n;
        }
        let value = |picks: &[usize]| island.evaluate(&variants, picks).map(|out| out[i]);
        let Some(here) = value(&picks) else {
            return self.describe(combination);
        };
        let same = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-15);
        let mut parts = Vec::new();
        for (k, (name, labels)) in self.variables.iter().enumerate() {
            if labels.len() < 2 {
                continue;
            }
            let matters = (0..labels.len()).filter(|&o| o != picks[k]).all(|o| {
                let mut other = picks.clone();
                other[k] = o;
                !matches!(value(&other), Some(v) if same(v, here))
            });
            if matters {
                parts.push(format!("{name} {}", labels[picks[k]]));
            }
        }
        parts.join(", ")
    }

    pub fn describe(&self, mut combination: u64) -> String {
        let mut parts = Vec::new();
        for (name, labels) in &self.variables {
            let n = labels.len() as u64;
            let k = (combination % n) as usize;
            combination /= n;
            if n > 1 {
                parts.push(format!("{name} {}", labels[k]));
            }
        }
        parts.join(", ")
    }
}

fn volts(v: f64) -> String {
    Quantity::point(Dimension::VOLTAGE, v).map_or_else(|_| format!("{v}"), |q| q.to_string())
}

fn amps(v: f64) -> String {
    Quantity::point(Dimension::CURRENT, v).map_or_else(|_| format!("{v}"), |q| q.to_string())
}

fn watts(v: f64) -> String {
    Quantity::point(Dimension::POWER, v)
        .map_or_else(|_| format!("{v}"), |q| q.to_string())
        .trim_end_matches('W')
        .to_string()
}

fn ohms(v: f64) -> String {
    Quantity::point(Dimension::RESISTANCE, v).map_or_else(|_| format!("{v}"), |q| q.to_string())
}

impl Island {
    pub fn nodes(&self) -> usize {
        self.free + self.held.len()
    }

    /// The variants of each variable: held nodes first, then elements.
    fn variants(&self) -> Vec<Vec<(String, Variant)>> {
        let mut all: Vec<Vec<(String, Variant)>> = Vec::new();
        for h in &self.held {
            all.push(
                h.v.corners()
                    .into_iter()
                    .map(|v| (format!("at {}", volts(v)), Variant::Held(v)))
                    .collect(),
            );
        }
        for e in &self.elements {
            all.push(match &e.kind {
                Kind::Resistor { a, b, r } => {
                    // A branch needs a node of its own to leave and one to enter, and one of
                    // them free.
                    let branch_ok = a != b && (*a < self.free || *b < self.free);
                    r.corners()
                        .into_iter()
                        .map(|r| {
                            let point = if r < MIN_RESISTANCE && branch_ok {
                                Point::Branch { a: *a, b: *b, r }
                            } else {
                                Point::R {
                                    a: *a,
                                    b: *b,
                                    r: r.max(MIN_RESISTANCE),
                                }
                            };
                            (format!("at {}", ohms(r)), Variant::Point(point))
                        })
                        .collect()
                }
                Kind::Current { from, to, i } => i
                    .corners()
                    .into_iter()
                    .map(|i| {
                        (
                            format!("at {}", amps(i)),
                            Variant::Point(Point::I {
                                from: *from,
                                to: *to,
                                i,
                            }),
                        )
                    })
                    .collect(),
                Kind::Regulator { from, to, draw } => match *draw {
                    Draw::Linear { i } => i
                        .corners()
                        .into_iter()
                        .map(|i| {
                            (
                                format!("supplying {}", amps(i)),
                                Variant::Point(Point::I {
                                    from: *from,
                                    to: *to,
                                    i,
                                }),
                            )
                        })
                        .collect(),
                    Draw::Switching { p, eff, iq } => {
                        let mut v = Vec::new();
                        for p in p.corners() {
                            for eff in eff.corners() {
                                for iq in iq.corners() {
                                    let own = if iq == 0.0 {
                                        String::new()
                                    } else {
                                        format!(" and keeping {}", amps(iq))
                                    };
                                    v.push((
                                        format!(
                                            "asking {} W out at {:.1} %{own}",
                                            watts(p),
                                            eff * 100.0
                                        ),
                                        Variant::Point(Point::Sink {
                                            from: *from,
                                            to: *to,
                                            p,
                                            eff,
                                            iq,
                                        }),
                                    ));
                                }
                            }
                        }
                        v
                    }
                },
                Kind::Diode { anode, cathode, vf } => {
                    let mut v = vec![(
                        "blocking".to_string(),
                        Variant::Point(Point::DiodeOff {
                            a: *anode,
                            k: *cathode,
                            vf: vf.hi,
                        }),
                    )];
                    for drop in vf.corners() {
                        v.push((
                            format!("conducting at {}", volts(drop)),
                            Variant::Point(Point::DiodeOn {
                                a: *anode,
                                k: *cathode,
                                vf: drop,
                            }),
                        ));
                    }
                    v
                }
                Kind::Mosfet {
                    gate,
                    drain,
                    source,
                    p_channel,
                    vth,
                    ron,
                } => {
                    let judge = Gate {
                        gate: *gate,
                        source: *source,
                        p_channel: *p_channel,
                        vth: *vth,
                    };
                    let mut v = vec![(
                        "off".to_string(),
                        Variant::Point(Point::Open {
                            a: *drain,
                            b: *source,
                            gate: Some(judge),
                        }),
                    )];
                    for r in ron.corners() {
                        v.push((
                            format!("on at {}", ohms(r)),
                            Variant::Point(Point::MosOn {
                                d: *drain,
                                s: *source,
                                r,
                                gate: judge,
                            }),
                        ));
                    }
                    v
                }
                Kind::Pin {
                    node,
                    high,
                    low,
                    z,
                    leak,
                } => {
                    let mut v = Vec::new();
                    for (word, side) in [("high", high), ("low", low)] {
                        let Some((to, r)) = side else { continue };
                        for r in r.corners() {
                            v.push((
                                format!("{word} through {}", ohms(r)),
                                Variant::Point(Point::R {
                                    a: *node,
                                    b: *to,
                                    r,
                                }),
                            ));
                        }
                    }
                    if *z {
                        v.push((
                            "high impedance".to_string(),
                            Variant::Point(Point::Open {
                                a: *node,
                                b: *node,
                                gate: None,
                            }),
                        ));
                        if let Some((pos, neg, r)) = leak {
                            for (word, to) in [("supply", pos), ("ground", neg)] {
                                for r in r.corners() {
                                    v.push((
                                        format!("leaking to the {word} through {}", ohms(r)),
                                        Variant::Point(Point::R {
                                            a: *node,
                                            b: *to,
                                            r,
                                        }),
                                    ));
                                }
                            }
                        }
                    }
                    v
                }
            });
        }
        all
    }

    /// The number of combinations, without building them.
    pub fn combinations(&self) -> u64 {
        let mut total: u64 = 1;
        for h in &self.held {
            total = total.saturating_mul(if h.v.is_point() { 1 } else { 2 });
        }
        for e in &self.elements {
            let n = match &e.kind {
                Kind::Resistor { r: v, .. } | Kind::Current { i: v, .. } => {
                    if v.is_point() {
                        1
                    } else {
                        2
                    }
                }
                Kind::Diode { vf, .. } => {
                    if vf.is_point() {
                        2
                    } else {
                        3
                    }
                }
                Kind::Regulator { draw, .. } => {
                    let two = |r: Range| if r.is_point() { 1 } else { 2 };
                    match *draw {
                        Draw::Linear { i } => two(i),
                        Draw::Switching { p, eff, iq } => two(p) * two(eff) * two(iq),
                    }
                }
                Kind::Mosfet { ron, .. } => 1 + if ron.is_point() { 1 } else { 2 },
                Kind::Pin {
                    high, low, z, leak, ..
                } => {
                    let two = |r: Range| if r.is_point() { 1 } else { 2 };
                    let leaking = leak.map_or(0, |(_, _, r)| 2 * two(r));
                    high.map_or(0, |(_, r)| two(r))
                        + low.map_or(0, |(_, r)| two(r))
                        + if *z { 1 + leaking } else { 0 }
                }
            };
            total = total.saturating_mul(n);
        }
        total
    }

    /// Solve every combination within the limit and collect the ranges.
    pub fn analyze(&self, limit: u64) -> Result<Report, Error> {
        let total = self.combinations();
        let size = (self.free
            + self
                .elements
                .iter()
                .filter(|e| match e.kind {
                    Kind::Diode { .. } => true,
                    Kind::Resistor { r, .. } => r.lo < MIN_RESISTANCE,
                    _ => false,
                })
                .count()) as u64;
        let rounds = if self.elements.iter().any(|e| {
            matches!(
                e.kind,
                Kind::Regulator {
                    draw: Draw::Switching { .. },
                    ..
                }
            )
        }) {
            ROUNDS_WORK
        } else {
            1
        };
        let limit = limit.min(MAX_WORK / ((size.pow(3) + 50) * rounds));
        if total > limit {
            return Err(Error::TooManyCombinations {
                combinations: total,
                limit,
            });
        }
        let variants = self.variants();
        let nodes = self.nodes();
        let n = self.elements.len();
        let quantities = nodes + 2 * n + self.held.len() + self.free;
        let mut collapses: Vec<Collapse> = Vec::new();
        let mut ranges = vec![Range::EMPTY; quantities];
        let mut at_lo = vec![0u64; quantities];
        let mut at_hi = vec![0u64; quantities];
        let held_count = self.held.len();
        let mut held_v = vec![0.0; held_count];
        let mut points: Vec<Point> = Vec::with_capacity(n);
        let mut work = Work::new(self.free, self.elements.len());
        let mut out = vec![0.0; quantities];
        let mut consistent = 0u64;
        let mut singular = false;
        for combination in 0..total {
            if combination % 256 == 0 {
                netsmith_core::job::checkpoint();
            }
            let mut c = combination;
            points.clear();
            for (k, v) in variants.iter().enumerate() {
                let m = v.len() as u64;
                let (_, variant) = &v[(c % m) as usize];
                c /= m;
                match variant {
                    Variant::Held(x) => held_v[k] = *x,
                    Variant::Point(p) => points.push(p.clone()),
                }
            }
            match self.operating_point(&points, &held_v, &mut work, &mut out) {
                Ok(Op::Point) => {
                    consistent += 1;
                    for (i, &v) in out.iter().enumerate() {
                        if v < ranges[i].lo {
                            at_lo[i] = combination;
                        }
                        if v > ranges[i].hi {
                            at_hi[i] = combination;
                        }
                        ranges[i].include(v);
                    }
                }
                Ok(Op::Refused) => {}
                Ok(Op::Collapse { element, vin }) => {
                    if !collapses.iter().any(|c| c.element == element) {
                        collapses.push(Collapse {
                            element,
                            combination,
                            vin,
                        });
                    }
                }
                Err(()) => singular = true,
            }
        }
        // A combination whose matrix cannot be solved says the circuit is not one this
        // analysis understands: no answer is better than a part of one.
        if singular {
            return Err(Error::Singular);
        }
        // Every combination that is the circuit's collapsed: the report says which, and has no
        // range to give.
        if consistent == 0 && collapses.is_empty() {
            return Err(Error::NoOperatingPoint);
        }
        let mut names: Vec<(String, Vec<String>)> = Vec::new();
        for (h, v) in self.held.iter().zip(&variants) {
            names.push((h.name.clone(), v.iter().map(|(l, _)| l.clone()).collect()));
        }
        for (e, v) in self.elements.iter().zip(&variants[held_count..]) {
            names.push((e.name.clone(), v.iter().map(|(l, _)| l.clone()).collect()));
        }
        Ok(Report {
            ranges,
            at_lo,
            at_hi,
            variables: names,
            nodes,
            elements: n,
            held: held_count,
            combinations: total,
            consistent,
            collapses,
        })
    }

    /// The quantities of one combination, given as the index of the variant of each variable,
    /// or `None` when the combination is refused or cannot be solved.
    fn evaluate(&self, variants: &[Vec<(String, Variant)>], picks: &[usize]) -> Option<Vec<f64>> {
        let held_count = self.held.len();
        let mut held_v = vec![0.0; held_count];
        let mut points = Vec::new();
        for (k, v) in variants.iter().enumerate() {
            match &v[picks[k]].1 {
                Variant::Held(x) => held_v[k] = *x,
                Variant::Point(p) => points.push(p.clone()),
            }
        }
        let mut work = Work::new(self.free, self.elements.len());
        let mut out = vec![0.0; self.nodes() + 2 * self.elements.len() + held_count + self.free];
        match self.operating_point(&points, &held_v, &mut work, &mut out) {
            Ok(Op::Point) => Some(out),
            _ => None,
        }
    }

    /// One operating point. `Op::Refused` when a state is not the solution's, `Err` when the
    /// matrix cannot be solved. `out` receives the node voltages (free, then held), then the
    /// current and the drop of each element, then what each held node gives into the elements.
    fn operating_point(
        &self,
        points: &[Point],
        held_v: &[f64],
        work: &mut Work,
        out: &mut [f64],
    ) -> Result<Op, ()> {
        let found = self.operating_point_of(points, held_v, work, out);
        // A combination whose branches make a matrix that cannot be solved (a loop of shorts
        // and sources, say two beads at zero ohm side by side) is solved with the branches
        // raised to the least conductance, as every resistance under it once was.
        if found.is_err() && points.iter().any(|p| matches!(p, Point::Branch { .. })) {
            let raised: Vec<Point> = points
                .iter()
                .map(|p| match *p {
                    Point::Branch { a, b, r } => Point::R {
                        a,
                        b,
                        r: r.max(MIN_RESISTANCE),
                    },
                    ref other => other.clone(),
                })
                .collect();
            return self.operating_point_of(&raised, held_v, work, out);
        }
        found
    }

    fn operating_point_of(
        &self,
        points: &[Point],
        held_v: &[f64],
        work: &mut Work,
        out: &mut [f64],
    ) -> Result<Op, ()> {
        let free = self.free;
        let on: Vec<usize> = (0..points.len())
            .filter(|&k| matches!(points[k], Point::DiodeOn { .. } | Point::Branch { .. }))
            .collect();
        let sinks: Vec<usize> = (0..points.len())
            .filter(|&k| matches!(points[k], Point::Sink { .. }))
            .collect();
        for d in work.draw.iter_mut() {
            *d = 0.0;
        }
        let volt = |x: &[f64], node: usize| {
            if node < free {
                x[node]
            } else {
                held_v[node - free]
            }
        };
        let mut x = self.solved(points, held_v, &on, work)?;
        if !sinks.is_empty() {
            // The fixed point: each input draws what its output's power asks at the voltage the
            // last round left it. From zero the rounds rise to the least fixed point, and fall
            // to nothing when there is none.
            let mut settled = false;
            let mut last = (sinks[0], 0.0);
            for round in 1..=MAX_ROUNDS {
                let mut worst = 0.0f64;
                let mut size = 0.0f64;
                for &k in &sinks {
                    let Point::Sink {
                        from,
                        to,
                        p,
                        eff,
                        iq,
                    } = points[k]
                    else {
                        continue;
                    };
                    let vin = volt(&x, from) - volt(&x, to);
                    last = (k, vin);
                    if vin < COLLAPSE_VOLTS {
                        return Ok(if self.states_ok(points, &on, &x, held_v) {
                            Op::Collapse { element: k, vin }
                        } else {
                            Op::Refused
                        });
                    }
                    let want = p / (eff * vin) + iq;
                    let step = if round > HALF_STEP_FROM { 0.5 } else { 1.0 };
                    worst = worst.max((want - work.draw[k]).abs());
                    size = size.max(want.abs());
                    work.next[k] = work.draw[k] + step * (want - work.draw[k]);
                }
                if worst <= 1e-12 + 1e-10 * size {
                    settled = true;
                    break;
                }
                for &k in &sinks {
                    work.draw[k] = work.next[k];
                }
                x = self.solved(points, held_v, &on, work)?;
            }
            if !settled {
                let (element, vin) = last;
                return Ok(if self.states_ok(points, &on, &x, held_v) {
                    Op::Collapse { element, vin }
                } else {
                    Op::Refused
                });
            }
        }
        if !self.states_ok(points, &on, &x, held_v) {
            return Ok(Op::Refused);
        }
        let v = |node: usize| volt(&x, node);
        let nodes = self.nodes();
        for (node, slot) in out.iter_mut().enumerate().take(nodes) {
            *slot = v(node);
        }
        let n = points.len();
        let branch_of = |k: usize| free + on.iter().position(|&o| o == k).unwrap_or(0);
        // The current of each element, from its first node to its second.
        let mut currents = vec![0.0; n];
        for (k, p) in points.iter().enumerate() {
            let (current, drop) = match *p {
                Point::R { a, b, r } => ((v(a) - v(b)) / r, v(a) - v(b)),
                Point::I { from, to, i } => (i, v(from) - v(to)),
                Point::Sink { from, to, .. } => (work.draw[k], v(from) - v(to)),
                Point::DiodeOn { a, k: cathode, .. } => {
                    (x[branch_of(k)].max(0.0), v(a) - v(cathode))
                }
                Point::Branch { a, b, .. } => (x[branch_of(k)], v(a) - v(b)),
                Point::DiodeOff { a, k: cathode, .. } => (0.0, v(a) - v(cathode)),
                Point::Open { a, b, .. } => (0.0, v(a) - v(b)),
                Point::MosOn { d, s, r, .. } => ((v(d) - v(s)) / r, v(d) - v(s)),
            };
            currents[k] = current;
            out[nodes + k] = current;
            out[nodes + n + k] = drop;
        }
        // What each held node gives: the sum of the currents that leave it into an element.
        for h in 0..self.held.len() {
            let node = free + h;
            let mut gives = 0.0;
            for (k, p) in points.iter().enumerate() {
                let (first, second) = match *p {
                    Point::R { a, b, .. } | Point::Branch { a, b, .. } => (a, b),
                    Point::I { from, to, .. } | Point::Sink { from, to, .. } => (from, to),
                    Point::DiodeOn { a, k: cathode, .. }
                    | Point::DiodeOff { a, k: cathode, .. } => (a, cathode),
                    Point::Open { a, b, .. } => (a, b),
                    Point::MosOn { d, s, .. } => (d, s),
                };
                if first == node {
                    gives += currents[k];
                }
                if second == node {
                    gives -= currents[k];
                }
            }
            out[nodes + 2 * n + h] = gives;
        }
        // What each free node takes in at this operating point: the sum of the currents that
        // enter it from an element (the same as what leaves it, by the law).
        let held_count = self.held.len();
        for node in 0..free {
            let mut enters = 0.0;
            for (k, p) in points.iter().enumerate() {
                let (first, second) = match *p {
                    Point::R { a, b, .. } | Point::Branch { a, b, .. } => (a, b),
                    Point::I { from, to, .. } | Point::Sink { from, to, .. } => (from, to),
                    Point::DiodeOn { a, k: cathode, .. }
                    | Point::DiodeOff { a, k: cathode, .. } => (a, cathode),
                    Point::Open { a, b, .. } => (a, b),
                    Point::MosOn { d, s, .. } => (d, s),
                };
                if first == node {
                    enters += (-currents[k]).max(0.0);
                }
                if second == node {
                    enters += currents[k].max(0.0);
                }
            }
            out[nodes + 2 * n + held_count + node] = enters;
        }
        Ok(Op::Point)
    }

    /// Whether the states are what the solution says: a diode that conducts carries a current
    /// of at least zero, one that blocks has no more than its drop across it.
    fn states_ok(&self, points: &[Point], on: &[usize], x: &[f64], held_v: &[f64]) -> bool {
        let free = self.free;
        let v = |node: usize| {
            if node < free {
                x[node]
            } else {
                held_v[node - free]
            }
        };
        // Which free nodes a conducting path joins to a held node. A MOSFET's gate that none
        // does has no voltage the circuit sets: it floats, and the switch may be either way.
        let joined = points
            .iter()
            .any(|p| matches!(p, Point::Open { gate: Some(_), .. } | Point::MosOn { .. }))
            .then(|| self.joined_to_held(points));
        let drive = |g: &Gate| {
            if g.gate < free && joined.as_ref().is_some_and(|j| !j[g.gate]) {
                return None;
            }
            let (gate, source) = (v(g.gate), v(g.source));
            Some(if g.p_channel {
                source - gate
            } else {
                gate - source
            })
        };
        points.iter().enumerate().all(|(k, p)| match *p {
            Point::DiodeOn { .. } => {
                let branch = free + on.iter().position(|&o| o == k).unwrap_or(0);
                x[branch] >= -CURRENT_SLACK
            }
            Point::DiodeOff { a, k: cathode, vf } => v(a) - v(cathode) <= vf + VOLTAGE_SLACK,
            Point::MosOn { gate, .. } => {
                drive(&gate).is_none_or(|vgs| vgs >= gate.vth.lo - VOLTAGE_SLACK)
            }
            Point::Open { gate: Some(g), .. } => {
                drive(&g).is_none_or(|vgs| vgs <= g.vth.hi + VOLTAGE_SLACK)
            }
            _ => true,
        })
    }

    /// For each node, whether a conducting element (a resistor, a diode that conducts, a
    /// MOSFET that is on, a pin that drives) joins it to a node that a source holds.
    fn joined_to_held(&self, points: &[Point]) -> Vec<bool> {
        let nodes = self.nodes();
        let free = self.free;
        let mut parent: Vec<usize> = (0..nodes).collect();
        fn find(parent: &mut [usize], x: usize) -> usize {
            let mut root = x;
            while parent[root] != root {
                root = parent[root];
            }
            let mut at = x;
            while parent[at] != root {
                let next = parent[at];
                parent[at] = root;
                at = next;
            }
            root
        }
        let join = |parent: &mut Vec<usize>, a: usize, b: usize| {
            let (ra, rb) = (find(parent, a), find(parent, b));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        };
        // Every held node is one class: a source holds it.
        for h in free + 1..nodes {
            join(&mut parent, free, h);
        }
        for p in points {
            match *p {
                Point::R { a, b, .. }
                | Point::Branch { a, b, .. }
                | Point::DiodeOn { a, k: b, .. } => join(&mut parent, a, b),
                Point::MosOn { d, s, .. } => join(&mut parent, d, s),
                _ => {}
            }
        }
        let held_class = if free < nodes {
            Some(find(&mut parent, free))
        } else {
            None
        };
        (0..nodes)
            .map(|n| Some(find(&mut parent, n)) == held_class)
            .collect()
    }

    /// The solution of one combination, with `gmin` only when it is singular without it.
    fn solved(
        &self,
        points: &[Point],
        held_v: &[f64],
        on: &[usize],
        work: &mut Work,
    ) -> Result<Vec<f64>, ()> {
        match self.stamp_and_solve(points, held_v, on, work, 0.0) {
            Some(x) => Ok(x),
            None => self
                .stamp_and_solve(points, held_v, on, work, GMIN)
                .ok_or(()),
        }
    }

    /// The matrix of one combination, with `gmin` from every free node to the ground, and its
    /// solution: the free node voltages, then the current of each conducting diode. The
    /// regulators' inputs draw the currents of the last round (`work.draw`).
    fn stamp_and_solve(
        &self,
        points: &[Point],
        held_v: &[f64],
        on: &[usize],
        work: &mut Work,
        gmin: f64,
    ) -> Option<Vec<f64>> {
        let free = self.free;
        let size = free + on.len();
        work.reset(size);
        let held = |node: usize| held_v[node - free];
        for (k, p) in points.iter().enumerate() {
            match *p {
                Point::R { a, b, r } => {
                    let g = 1.0 / r;
                    for (x, y) in [(a, b), (b, a)] {
                        if x < free {
                            work.add(x, x, g);
                            if y < free {
                                work.add(x, y, -g);
                            } else {
                                work.rhs[x] += g * held(y);
                            }
                        }
                    }
                }
                Point::I { from, to, i } => {
                    if from < free {
                        work.rhs[from] -= i;
                    }
                    if to < free {
                        work.rhs[to] += i;
                    }
                }
                Point::Sink { from, to, .. } => {
                    let i = work.draw[k];
                    if from < free {
                        work.rhs[from] -= i;
                    }
                    if to < free {
                        work.rhs[to] += i;
                    }
                }
                Point::Branch { a, b: cathode, r } => {
                    let branch = free + on.iter().position(|&o| o == k).unwrap_or(0);
                    // The current leaves `a` and enters `cathode`; `V(a) - V(cathode) = r I`.
                    if a < free {
                        work.add(a, branch, 1.0);
                        work.add(branch, a, 1.0);
                    } else {
                        work.rhs[branch] -= held(a);
                    }
                    if cathode < free {
                        work.add(cathode, branch, -1.0);
                        work.add(branch, cathode, -1.0);
                    } else {
                        work.rhs[branch] += held(cathode);
                    }
                    work.add(branch, branch, -r);
                }
                Point::DiodeOn { a, k: cathode, vf } => {
                    let branch = free + on.iter().position(|&o| o == k).unwrap_or(0);
                    // The branch current leaves `a` and enters `cathode`.
                    if a < free {
                        work.add(a, branch, 1.0);
                        work.add(branch, a, 1.0);
                    } else {
                        work.rhs[branch] -= held(a);
                    }
                    if cathode < free {
                        work.add(cathode, branch, -1.0);
                        work.add(branch, cathode, -1.0);
                    } else {
                        work.rhs[branch] += held(cathode);
                    }
                    work.rhs[branch] += vf;
                }
                Point::DiodeOff { .. } | Point::Open { .. } => {}
                Point::MosOn { d, s, r, .. } => {
                    let g = 1.0 / r;
                    for (x, y) in [(d, s), (s, d)] {
                        if x < free {
                            work.add(x, x, g);
                            if y < free {
                                work.add(x, y, -g);
                            } else {
                                work.rhs[x] += g * held(y);
                            }
                        }
                    }
                }
            }
        }
        // The conductance to the ground is added last, and is at least a fraction of the largest
        // conductance on a diagonal that the rounding of a double keeps: a picoamp per megavolt
        // is lost against the 30 S of a fuse (30 + 1e-15 is 30), and a cluster of nodes that a
        // blocking diode leaves to a few current sources (the loads of parts still to be picked)
        // would stay singular.
        if gmin > 0.0 {
            let stride = work.stride;
            let big = (0..free).fold(0.0f64, |m, row| m.max(work.m[row * stride + row]));
            let g = gmin.max(1e-12 * big);
            for row in 0..free {
                work.add(row, row, g);
            }
        }
        // Without `gmin` a pivot that is rounding and not a conductance (a cluster of nodes that
        // nothing holds, behind a diode that blocks) is as good as zero: the solution of such a
        // matrix is a number of the size of 1 / rounding, and must not pass for a voltage.
        work.solve(size, if gmin > 0.0 { 0.0 } else { SINGULAR })
    }
}

enum Variant {
    Held(f64),
    Point(Point),
}

/// The matrix and the right side of one solve, kept between combinations.
struct Work {
    m: Vec<f64>,
    rhs: Vec<f64>,
    stride: usize,
    /// The current each regulator's input draws in the round, by point, and the next round's.
    draw: Vec<f64>,
    next: Vec<f64>,
}

impl Work {
    fn new(free: usize, elements: usize) -> Work {
        let stride = free + elements;
        Work {
            m: vec![0.0; stride * stride],
            rhs: vec![0.0; stride],
            stride,
            draw: vec![0.0; elements],
            next: vec![0.0; elements],
        }
    }

    fn reset(&mut self, size: usize) {
        for i in 0..size * self.stride {
            self.m[i] = 0.0;
        }
        for r in self.rhs.iter_mut() {
            *r = 0.0;
        }
    }

    fn add(&mut self, row: usize, col: usize, v: f64) {
        self.m[row * self.stride + col] += v;
    }

    /// Gaussian elimination with partial pivoting, on the first `size` rows. A pivot below
    /// `tiny` times the largest entry of the matrix is zero.
    fn solve(&mut self, size: usize, tiny: f64) -> Option<Vec<f64>> {
        let s = self.stride;
        let mut scale = 0.0f64;
        for row in 0..size {
            for k in 0..size {
                scale = scale.max(self.m[row * s + k].abs());
            }
        }
        let threshold = (tiny * scale).max(1e-30);
        for col in 0..size {
            let mut pivot = col;
            for row in col + 1..size {
                if self.m[row * s + col].abs() > self.m[pivot * s + col].abs() {
                    pivot = row;
                }
            }
            if self.m[pivot * s + col].abs() < threshold {
                return None;
            }
            if pivot != col {
                for k in 0..size {
                    self.m.swap(col * s + k, pivot * s + k);
                }
                self.rhs.swap(col, pivot);
            }
            for row in col + 1..size {
                let f = self.m[row * s + col] / self.m[col * s + col];
                if f == 0.0 {
                    continue;
                }
                for k in col..size {
                    let sub = f * self.m[col * s + k];
                    self.m[row * s + k] -= sub;
                }
                let sub = f * self.rhs[col];
                self.rhs[row] -= sub;
            }
        }
        let mut x = vec![0.0; size];
        for row in (0..size).rev() {
            let mut sum = self.rhs[row];
            for (k, xk) in x.iter().enumerate().skip(row + 1) {
                sum -= self.m[row * s + k] * xk;
            }
            x[row] = sum / self.m[row * s + row];
        }
        Some(x)
    }
}

/// Draw `samples` points inside the ranges (and every state of the diodes) and count the
/// results that fall outside the report's ranges, by quantity. Zero is evidence that the hull
/// of the corners covers the box, not proof. For tests: a fixed seed, so every run draws the
/// same points.
#[cfg(test)]
pub(crate) fn check_interior(island: &Island, report: &Report, samples: usize) -> usize {
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rng = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let free = island.free;
    let mut work = Work::new(free, island.elements.len());
    let mut out =
        vec![0.0; island.nodes() + 2 * island.elements.len() + island.held.len() + island.free];
    let mut outside = 0;
    for _ in 0..samples {
        let held_v: Vec<f64> = island
            .held
            .iter()
            .map(|h| h.v.lo + (h.v.hi - h.v.lo) * rng())
            .collect();
        let points: Vec<Point> = island
            .elements
            .iter()
            .map(|e| match &e.kind {
                Kind::Resistor { a, b, r } => {
                    let r = r.lo + (r.hi - r.lo) * rng();
                    if r < MIN_RESISTANCE && a != b && (*a < free || *b < free) {
                        Point::Branch { a: *a, b: *b, r }
                    } else {
                        Point::R {
                            a: *a,
                            b: *b,
                            r: r.max(MIN_RESISTANCE),
                        }
                    }
                }
                Kind::Current { from, to, i } => Point::I {
                    from: *from,
                    to: *to,
                    i: i.lo + (i.hi - i.lo) * rng(),
                },
                Kind::Regulator { from, to, draw } => match *draw {
                    Draw::Linear { i } => Point::I {
                        from: *from,
                        to: *to,
                        i: i.lo + (i.hi - i.lo) * rng(),
                    },
                    Draw::Switching { p, eff, iq } => Point::Sink {
                        from: *from,
                        to: *to,
                        p: p.lo + (p.hi - p.lo) * rng(),
                        eff: eff.lo + (eff.hi - eff.lo) * rng(),
                        iq: iq.lo + (iq.hi - iq.lo) * rng(),
                    },
                },
                Kind::Mosfet {
                    gate,
                    drain,
                    source,
                    p_channel,
                    vth,
                    ron,
                } => {
                    let judge = Gate {
                        gate: *gate,
                        source: *source,
                        p_channel: *p_channel,
                        vth: *vth,
                    };
                    if rng() < 0.5 {
                        Point::MosOn {
                            d: *drain,
                            s: *source,
                            r: ron.lo + (ron.hi - ron.lo) * rng(),
                            gate: judge,
                        }
                    } else {
                        Point::Open {
                            a: *drain,
                            b: *source,
                            gate: Some(judge),
                        }
                    }
                }
                Kind::Pin {
                    node,
                    high,
                    low,
                    z,
                    leak,
                } => {
                    let mut states: Vec<Point> = Vec::new();
                    for (to, r) in [high, low].into_iter().flatten() {
                        states.push(Point::R {
                            a: *node,
                            b: *to,
                            r: r.lo + (r.hi - r.lo) * rng(),
                        });
                    }
                    if *z {
                        states.push(Point::Open {
                            a: *node,
                            b: *node,
                            gate: None,
                        });
                        if let Some((pos, neg, r)) = leak {
                            for to in [pos, neg] {
                                states.push(Point::R {
                                    a: *node,
                                    b: *to,
                                    r: r.lo + (r.hi - r.lo) * rng(),
                                });
                            }
                        }
                    }
                    let pick = ((rng() * states.len() as f64) as usize).min(states.len() - 1);
                    states.swap_remove(pick)
                }
                Kind::Diode { anode, cathode, vf } => {
                    let vf = vf.lo + (vf.hi - vf.lo) * rng();
                    if rng() < 0.5 {
                        Point::DiodeOn {
                            a: *anode,
                            k: *cathode,
                            vf,
                        }
                    } else {
                        Point::DiodeOff {
                            a: *anode,
                            k: *cathode,
                            vf,
                        }
                    }
                }
            })
            .collect();
        if let Ok(Op::Point) = island.operating_point(&points, &held_v, &mut work, &mut out) {
            for (i, &v) in out.iter().enumerate() {
                let r = report.ranges[i];
                let slack = 1e-9 * r.lo.abs().max(r.hi.abs()) + 1e-15;
                if v < r.lo - slack || v > r.hi + slack {
                    outside += 1;
                }
            }
        }
    }
    outside
}
