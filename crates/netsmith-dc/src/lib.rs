//! A feasibility prototype of a post-pick DC operating-point analysis (ADR 0076, proposed).
//!
//! netsmith's solver propagates ranges through equations and adds the currents of a net
//! (ADR 0007, ADR 0036). It never builds the network of the picked parts and solves it. This
//! crate is that step on its own, so that its cost and its results can be judged before anyone
//! decides to wire it in: nothing in the workspace depends on it, and `netsmith build` does not
//! run it.
//!
//! - [`netlist`]: nodes and elements (a resistor with a tolerance, a rail with a range, a
//!   diode with a forward drop, a logic pin that is high, low or high impedance, a MOSFET as a
//!   switch, a regulator that conserves power, a load of constant power).
//! - [`op`]: one operating point by modified nodal analysis, with a fixed-point iteration for
//!   the elements whose current depends on their own voltage.
//! - [`corners`]: every corner of the ranges and every state of the elements, solved, and the
//!   range each voltage, current and power takes.
//! - [`text`]: a one-line-per-element format for islands.

pub mod corners;
pub mod netlist;
pub mod op;
pub mod range;
pub mod text;

pub use corners::{Report, analyze, check_interior};
pub use netlist::{Kind, Logic, LogicState, Netlist, Regulator};
pub use range::Range;
