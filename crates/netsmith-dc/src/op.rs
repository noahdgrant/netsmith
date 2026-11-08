//! One DC operating point of a netlist whose every value is fixed: modified nodal analysis, and a
//! fixed-point iteration for the elements whose current depends on their own voltage (a
//! regulator's input, a load of constant power).
//!
//! The unknowns are the node voltages (the ground is the reference) and one current for each
//! element that holds a voltage between two nodes (a source, a conducting diode, a regulator's
//! output). Each equation is Kirchhoff's current law at a node, or the voltage an element holds.

use crate::netlist::{GROUND, Point};

/// A conductance from every node to ground, as SPICE does, so that a node nothing drives (a
/// pin that goes nowhere) has a voltage of zero and not a singular matrix. It is 1e-12 S, a
/// leak of a picoampere per volt: 1e-6 of the current of a 1 Mohm resistor.
pub const GMIN: f64 = 1e-12;

/// The most iterations of the fixed point, and the iteration after which it takes half steps.
const MAX_ITERATIONS: usize = 400;
const HALF_STEP_FROM: usize = 60;

/// A voltage below this at the input of a regulator or a constant-power load is a collapse:
/// the load asks for more power than the source can give through the resistance in between.
const COLLAPSE_VOLTS: f64 = 1e-3;

/// How far a state may be wrong before it is refused: a diode that conducts must carry a
/// current of at least `-1 nA`, one that blocks may have `1 nV` across it the wrong way.
const STATE_SLACK: f64 = 1e-9;

#[derive(Debug, PartialEq)]
pub enum Fail {
    /// The equations have no solution: a loop of voltage sources, for example.
    Singular,
    /// The states chosen (a diode conducting, a switch off) are not what the solution says.
    Inconsistent,
    /// The sources cannot give what a constant-power load asks for, or the iteration did not
    /// settle. The text says which.
    Collapse(String),
}

pub struct Op {
    /// The voltage of each node but the ground, then the quantities of each element in the
    /// order of `Element::quantity_names`.
    pub values: Vec<f64>,
    pub iterations: usize,
    /// The indices (into the points) of the regulators whose input is below their minimum.
    pub below_min: Vec<usize>,
}

fn has_branch(p: &Point) -> bool {
    matches!(
        p,
        Point::V { .. } | Point::DiodeOn { .. } | Point::Reg { .. }
    )
}

fn is_sink(p: &Point) -> bool {
    matches!(p, Point::Reg { .. } | Point::PLoad { .. })
}

struct System {
    m: Vec<Vec<f64>>,
    rhs: Vec<f64>,
}

impl System {
    fn new(size: usize) -> System {
        System {
            m: vec![vec![0.0; size]; size],
            rhs: vec![0.0; size],
        }
    }

    fn row(node: usize) -> Option<usize> {
        node.checked_sub(1)
    }

    fn conductance(&mut self, a: usize, b: usize, g: f64) {
        let (ra, rb) = (Self::row(a), Self::row(b));
        if let Some(i) = ra {
            self.m[i][i] += g;
        }
        if let Some(j) = rb {
            self.m[j][j] += g;
        }
        if let (Some(i), Some(j)) = (ra, rb) {
            self.m[i][j] -= g;
            self.m[j][i] -= g;
        }
    }

    /// A current `i` taken out of `from` and put into `to`.
    fn current(&mut self, from: usize, to: usize, i: f64) {
        if let Some(r) = Self::row(from) {
            self.rhs[r] -= i;
        }
        if let Some(r) = Self::row(to) {
            self.rhs[r] += i;
        }
    }

    /// `V(pos) - V(neg) = v`, with the current through the source from `pos` to `neg` in
    /// column `branch`.
    fn voltage(&mut self, pos: usize, neg: usize, v: f64, branch: usize) {
        if let Some(r) = Self::row(pos) {
            self.m[r][branch] += 1.0;
            self.m[branch][r] += 1.0;
        }
        if let Some(r) = Self::row(neg) {
            self.m[r][branch] -= 1.0;
            self.m[branch][r] -= 1.0;
        }
        self.rhs[branch] = v;
    }

    /// Gaussian elimination with partial pivoting.
    fn solve(mut self) -> Result<Vec<f64>, Fail> {
        let n = self.rhs.len();
        for col in 0..n {
            let pivot = (col..n)
                .max_by(|&a, &b| self.m[a][col].abs().total_cmp(&self.m[b][col].abs()))
                .unwrap_or(col);
            if self.m[pivot][col].abs() < 1e-30 {
                return Err(Fail::Singular);
            }
            self.m.swap(col, pivot);
            self.rhs.swap(col, pivot);
            for row in col + 1..n {
                let f = self.m[row][col] / self.m[col][col];
                if f == 0.0 {
                    continue;
                }
                for k in col..n {
                    let sub = f * self.m[col][k];
                    self.m[row][k] -= sub;
                }
                let sub = f * self.rhs[col];
                self.rhs[row] -= sub;
            }
        }
        let mut x = vec![0.0; n];
        for row in (0..n).rev() {
            let mut s = self.rhs[row];
            for (k, &xk) in x.iter().enumerate().skip(row + 1) {
                s -= self.m[row][k] * xk;
            }
            x[row] = s / self.m[row][row];
        }
        Ok(x)
    }
}

/// The solution of the linear network with the sinks' currents fixed.
struct Linear {
    /// Voltage of every node, the ground first.
    v: Vec<f64>,
    /// The branch current of each point that has one.
    branch: Vec<f64>,
}

fn linear(
    points: &[Point],
    n_nodes: usize,
    branch_of: &[Option<usize>],
    n_branch: usize,
    sinks: &[f64],
) -> Result<Linear, Fail> {
    let rows = n_nodes - 1;
    let mut s = System::new(rows + n_branch);
    for node in 1..n_nodes {
        s.conductance(node, GROUND, GMIN);
    }
    for (k, p) in points.iter().enumerate() {
        let branch = branch_of[k].map(|b| rows + b);
        match *p {
            Point::R { a, b, r } => s.conductance(a, b, 1.0 / r),
            Point::V { pos, neg, v } => s.voltage(pos, neg, v, branch.unwrap_or(0)),
            Point::I { from, to, i } => s.current(from, to, i),
            Point::DiodeOn { a, k: cathode, vf } => {
                s.voltage(a, cathode, vf, branch.unwrap_or(0));
            }
            Point::DiodeOff { .. } | Point::MosOff { .. } => {}
            Point::Drive { node, to, r } => s.conductance(node, to, 1.0 / r),
            Point::Leak { node, gnd, i } => s.current(node, gnd, i),
            Point::MosOn {
                drain, source, ron, ..
            } => s.conductance(drain, source, 1.0 / ron),
            Point::Reg {
                vin, vout, gnd, v, ..
            } => {
                s.voltage(vout, gnd, v, branch.unwrap_or(0));
                s.current(vin, gnd, sinks[k]);
            }
            Point::PLoad { node, gnd, .. } => s.current(node, gnd, sinks[k]),
        }
    }
    let x = s.solve()?;
    let mut v = vec![0.0; n_nodes];
    v[1..n_nodes].copy_from_slice(&x[..rows]);
    let branch = (0..points.len())
        .map(|k| branch_of[k].map_or(0.0, |b| x[rows + b]))
        .collect();
    Ok(Linear { v, branch })
}

/// The current a sink draws, given the network solved with the last guess of it.
fn sink_current(p: &Point, sol: &Linear, k: usize) -> Result<f64, Fail> {
    match *p {
        Point::Reg {
            vin,
            gnd,
            v,
            eff,
            iq,
            linear,
            ..
        } => {
            let out = (-sol.branch[k]).max(0.0);
            let vin_v = sol.v[vin] - sol.v[gnd];
            if linear {
                return Ok(iq + out);
            }
            if vin_v < COLLAPSE_VOLTS {
                return Err(Fail::Collapse(format!(
                    "the input of a regulator fell to {vin_v:.3} V: the source cannot give the power its output asks for"
                )));
            }
            Ok(iq + v * out / (eff * vin_v))
        }
        Point::PLoad { node, gnd, p } => {
            let v = sol.v[node] - sol.v[gnd];
            if v < COLLAPSE_VOLTS {
                return Err(Fail::Collapse(format!(
                    "the voltage of a constant-power load fell to {v:.3} V: the source cannot give {p} W"
                )));
            }
            Ok(p / v)
        }
        _ => Ok(0.0),
    }
}

/// Whether the states chosen (a diode conducting or blocking, a switch on or off) are what the
/// solution says: a diode that conducts carries a current, one that blocks has less across it
/// than its drop, a switch that is on has a gate-source voltage above the least threshold and
/// one that is off a voltage below the largest.
fn consistent(points: &[Point], sol: &Linear) -> bool {
    let vd = |a: usize, b: usize| sol.v[a] - sol.v[b];
    let vgs = |gate: usize, source: usize, p_channel: bool| {
        if p_channel {
            vd(source, gate)
        } else {
            vd(gate, source)
        }
    };
    points.iter().enumerate().all(|(k, p)| match *p {
        Point::DiodeOn { .. } => sol.branch[k] >= -STATE_SLACK,
        Point::DiodeOff { a, k: c, vf } => vd(a, c) <= vf + STATE_SLACK,
        Point::MosOn {
            gate,
            source,
            p_channel,
            vth,
            ..
        } => vgs(gate, source, p_channel) >= vth.lo - STATE_SLACK,
        Point::MosOff {
            gate,
            source,
            p_channel,
            vth,
        } => vgs(gate, source, p_channel) <= vth.hi + STATE_SLACK,
        _ => true,
    })
}

/// A collapse that comes of states that are not consistent (a blocking diode in front of a
/// regulator leaves it no input) is not a finding: those states are not the circuit's.
fn collapse(points: &[Point], sol: &Linear, why: String) -> Fail {
    if consistent(points, sol) {
        Fail::Collapse(why)
    } else {
        Fail::Inconsistent
    }
}

/// Solve one point netlist. `n_nodes` counts the ground.
pub fn solve(points: &[Point], n_nodes: usize) -> Result<Op, Fail> {
    let mut branch_of = vec![None; points.len()];
    let mut n_branch = 0;
    for (k, p) in points.iter().enumerate() {
        if has_branch(p) {
            branch_of[k] = Some(n_branch);
            n_branch += 1;
        }
    }

    let mut sinks = vec![0.0; points.len()];
    let mut iterations = 1;
    let mut sol = linear(points, n_nodes, &branch_of, n_branch, &sinks)?;
    if points.iter().any(is_sink) {
        let mut settled = false;
        for it in 1..=MAX_ITERATIONS {
            let mut worst = 0.0f64;
            let mut size = 0.0f64;
            let mut next = sinks.clone();
            for (k, p) in points.iter().enumerate() {
                if is_sink(p) {
                    let want = match sink_current(p, &sol, k) {
                        Ok(w) => w,
                        Err(Fail::Collapse(why)) => return Err(collapse(points, &sol, why)),
                        Err(e) => return Err(e),
                    };
                    let step = if it > HALF_STEP_FROM { 0.5 } else { 1.0 };
                    next[k] = sinks[k] + step * (want - sinks[k]);
                    worst = worst.max((want - sinks[k]).abs());
                    size = size.max(want.abs());
                }
            }
            iterations = it;
            if worst <= 1e-12 + 1e-10 * size {
                settled = true;
                break;
            }
            sinks = next;
            sol = linear(points, n_nodes, &branch_of, n_branch, &sinks)?;
        }
        if !settled {
            return Err(collapse(
                points,
                &sol,
                "the currents of the regulators and loads did not settle".to_string(),
            ));
        }
    }

    if !consistent(points, &sol) {
        return Err(Fail::Inconsistent);
    }
    let mut values: Vec<f64> = sol.v[1..].to_vec();
    let mut below_min = Vec::new();
    for (k, p) in points.iter().enumerate() {
        let vd = |a: usize, b: usize| sol.v[a] - sol.v[b];
        match *p {
            Point::R { a, b, r } => {
                let i = vd(a, b) / r;
                values.extend([i, i * vd(a, b)]);
            }
            Point::V { .. } => values.push(-sol.branch[k]),
            Point::I { i, .. } => values.push(i),
            Point::DiodeOn { vf, .. } => {
                let i = sol.branch[k];
                values.extend([i, vf * i]);
            }
            Point::DiodeOff { .. } | Point::MosOff { .. } => values.extend([0.0, 0.0]),
            Point::Drive { node, to, r } => values.push(vd(to, node) / r),
            Point::Leak { i, .. } => values.push(-i),
            Point::MosOn {
                drain, source, ron, ..
            } => {
                let i = vd(drain, source) / ron;
                values.extend([i, i * i * ron]);
            }
            Point::Reg {
                vin,
                vout,
                gnd,
                vin_min,
                ..
            } => {
                let out = -sol.branch[k];
                let loss = vd(vin, gnd) * sinks[k] - vd(vout, gnd) * out;
                if vin_min > 0.0 && vd(vin, gnd) < vin_min {
                    below_min.push(k);
                }
                values.extend([sinks[k], out, loss]);
            }
            Point::PLoad { node, gnd, .. } => {
                let i = sinks[k];
                values.extend([i, i * vd(node, gnd)]);
            }
        }
    }
    // A power or a current of 1e-15 is the rounding of the elimination, not a quantity.
    for v in values.iter_mut() {
        if v.abs() < 1e-15 {
            *v = 0.0;
        }
    }
    Ok(Op {
        values,
        iterations,
        below_min,
    })
}
