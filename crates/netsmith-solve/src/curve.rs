//! Curve files: a part's value as points over its operating conditions, read from a file
//! beside the catalog and interpolated where the design puts the part (issue #1260).
//!
//! A catalog row holds one figure for each column: a diode's forward voltage at its rated
//! current, a regulator's dropout at its full load. A datasheet draws more: the forward
//! voltage at each current and temperature. A curve file holds those points for one row and
//! one column, and the solver reads the column off it at the current and the temperature the
//! design gives the part. The row keeps its single figure, which stands for the part where the
//! design gives no operating point.
//!
//! # The file
//!
//! `curves/<catalog>/<row>.csv` in the folder of the catalog file (`data/jlcpcb/catalog/` for
//! the standard catalogs, a `catalog_dirs` folder or the folder of a file `catalogs` lists for
//! a project), named by the row it is for: its `lcsc` number, or its `mpn` where it has none.
//! A file whose name is no row's is an error that names it. A project's curve may be for a
//! standard row. The first line names the columns: every column but the last is an axis, and
//! the last is the catalog column the points give.
//!
//! ```text
//! temperature,current,forward_voltage
//! 25degC,1mA,0.30V
//! 25degC,100mA,0.42V
//! 25degC,1A,0.55V
//! 125degC,1mA,0.18V
//! 125degC,1A,0.45V
//! ```
//!
//! - **An axis** is a field of the part, named by its path without the dot (`current`), or
//!   `temperature`, the temperature of the board's `.environment` (ADR 0099). Each cell of an
//!   axis is one value in the language's syntax with its unit (`100mA`, `-40degC`), and every
//!   cell of a column has the same dimension.
//! - **The value** may be one value or a range (`0.40V..0.45V`), so a file may give the
//!   datasheet's typical curve or its bounds. Its dimension is the column's.
//! - **The points** are grouped by the earlier axes: the lines with the same temperature are
//!   one curve over the current, as a datasheet draws one line for each temperature, and each
//!   group may have currents of its own. The lines may come in any order; the same point
//!   twice is an error.
//!
//! # Reading it
//!
//! Between two points the value is a straight line in each axis: along the last axis inside a
//! group, then between the groups of the earlier axis. Over a range of each axis (a current of
//! `10mA..200mA`, an environment of `-40degC..85degC`) the value is the lowest and the highest
//! the lines reach, which are at the ends of the ranges or at points inside them, so the
//! answer is an interval that holds every value the file gives over the ranges. A range that
//! goes past what the file covers on any axis has no answer: the part is never assumed to
//! behave beyond its points (as a capacitor's bias curve is not, ADR 0013). Straight lines
//! between points are what the file says; a curve that bends between two points is read as
//! the chord, so a file gives points close enough for the use.
//!
//! [`Curve`] is the reader; the solver binds its axes to the part (`crate::solve`) and the
//! catalog files attach it to a row (`crate::table`).

use std::cmp::Ordering;

use netsmith_quantity::{Dimension, Quantity};

/// The name of the axis that is the board's environment temperature, not a field of the part.
pub const TEMPERATURE: &str = "temperature";

/// The folder beside a catalog file that holds its curve files, one folder for each catalog.
pub const FOLDER: &str = "curves";

/// The points of one curve file.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// What messages call the file (`catalogs/curves/schottky/C2480.csv`).
    pub file: String,
    /// The axes, in the order of the file's columns, each with its dimension.
    pub axes: Vec<(String, Dimension)>,
    /// The catalog column the points give.
    pub column: String,
    /// The dimension of the value.
    pub dim: Dimension,
    root: Node,
}

/// A curve over the axes from one on: a group for each value of the first, or the value.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Value(f64, f64),
    Axis(Vec<(f64, Node)>),
}

/// Why a curve has no value over the ranges asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Beyond {
    /// The axis whose range goes past the points.
    pub axis: String,
    pub dim: Dimension,
    /// The range asked, and the range the points cover there.
    pub wanted: (f64, f64),
    pub known: (f64, f64),
}

impl std::fmt::Display for Beyond {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let show = |(lo, hi): (f64, f64)| {
            Quantity::interval(self.dim, lo, hi)
                .map_or_else(|_| format!("{lo}..{hi}"), |q| q.to_string())
        };
        write!(
            f,
            "its `{}` is {}, and the curve covers {}",
            self.axis,
            show(self.wanted),
            show(self.known)
        )
    }
}

impl Curve {
    /// Read one curve file. `file` is what messages call it. Every problem names the file and
    /// line.
    pub fn parse(file: &str, text: &str) -> Result<Curve, String> {
        let records = crate::table::parse_csv(text).map_err(|e| format!("{file}:{e}"))?;
        let Some(((line, header), body)) = records.split_first() else {
            return Err(format!(
                "{file}: the file is empty; the first line names the axes and then the column the points give (`current,forward_voltage`)"
            ));
        };
        let names: Vec<String> = header.iter().map(|n| n.trim().to_string()).collect();
        if names.len() < 2 || names.iter().any(String::is_empty) {
            return Err(format!(
                "{file}:{line}: the first line names one axis or more and then the column the points give (`current,forward_voltage`), each with a name"
            ));
        }
        if let Some(twice) = names
            .iter()
            .enumerate()
            .find_map(|(i, n)| names[..i].contains(n).then_some(n))
        {
            return Err(format!(
                "{file}:{line}: the column `{twice}` is named twice"
            ));
        }
        let mut dims: Vec<Option<Dimension>> = vec![None; names.len()];
        let mut points: Vec<(usize, Vec<f64>, (f64, f64))> = Vec::new();
        for (line, cells) in body {
            if cells.len() != names.len() {
                return Err(format!(
                    "{file}:{line}: {} cells, and the first line names {}",
                    cells.len(),
                    names.len()
                ));
            }
            let mut at = Vec::with_capacity(names.len() - 1);
            let mut value = (0.0, 0.0);
            for (i, cell) in cells.iter().enumerate() {
                let cell = cell.trim();
                let bad = |why: String| format!("{file}:{line}: `{}`: {why}", names[i]);
                let q = crate::table::quantity(cell).map_err(bad)?;
                match dims[i] {
                    None => dims[i] = Some(q.dim()),
                    Some(d) if d != q.dim() => {
                        return Err(bad(format!(
                            "`{cell}` is in {}, and the column's first point is in {d}",
                            q.dim()
                        )));
                    }
                    Some(_) => {}
                }
                if i + 1 == names.len() {
                    value = (q.lo(), q.hi());
                } else if (q.hi() - q.lo()).abs() > q.lo().abs().max(q.hi().abs()) * 1e-12 {
                    return Err(bad(format!(
                        "`{cell}` is a range, and a point of an axis is one value"
                    )));
                } else {
                    at.push(q.lo());
                }
            }
            points.push((*line, at, value));
        }
        if points.is_empty() {
            return Err(format!(
                "{file}: the file has no points after its first line"
            ));
        }
        points.sort_by(|a, b| compare(&a.1, &b.1));
        if let Some(pair) = points
            .windows(2)
            .find(|p| compare(&p[0].1, &p[1].1).is_eq())
        {
            return Err(format!(
                "{file}:{}: the same point as line {}",
                pair[1].0.max(pair[0].0),
                pair[1].0.min(pair[0].0)
            ));
        }
        let entries: Vec<(&[f64], (f64, f64))> = points
            .iter()
            .map(|(_, at, v)| (at.as_slice(), *v))
            .collect();
        let dims: Vec<Dimension> = dims
            .into_iter()
            .map(|d| d.expect("every column has a point"))
            .collect();
        let column = names.last().expect("two names or more").clone();
        Ok(Curve {
            file: file.to_string(),
            axes: names[..names.len() - 1]
                .iter()
                .cloned()
                .zip(dims.iter().copied())
                .collect(),
            column,
            dim: dims[dims.len() - 1],
            root: build(&entries),
        })
    }

    /// The lowest and the highest value of every axis over all the points.
    pub fn span(&self, axis: usize) -> (f64, f64) {
        fn walk(node: &Node, depth: usize, axis: usize, out: &mut (f64, f64)) {
            if let Node::Axis(groups) = node {
                for (x, sub) in groups {
                    if depth == axis {
                        out.0 = out.0.min(*x);
                        out.1 = out.1.max(*x);
                    } else {
                        walk(sub, depth + 1, axis, out);
                    }
                }
            }
        }
        let mut out = (f64::INFINITY, f64::NEG_INFINITY);
        walk(&self.root, 0, axis, &mut out);
        out
    }

    /// The value over a range of each axis, in the order of [`Curve::axes`]: the interval
    /// from the lowest to the highest value the curve has there.
    pub fn over(&self, ranges: &[(f64, f64)]) -> Result<Quantity, Beyond> {
        assert_eq!(ranges.len(), self.axes.len(), "one range for each axis");
        // The lines are at their extremes at the ends of the ranges or at a point inside
        // them, in every axis: try each such corner.
        let marks: Vec<Vec<f64>> = ranges
            .iter()
            .enumerate()
            .map(|(axis, &(lo, hi))| {
                let mut marks = vec![lo, hi];
                self.coordinates(axis, &mut marks, lo, hi);
                marks
            })
            .collect();
        let (mut least, mut most) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut corner = vec![0.0; marks.len()];
        let mut index = vec![0usize; marks.len()];
        loop {
            for (axis, &i) in index.iter().enumerate() {
                corner[axis] = marks[axis][i];
            }
            let (lo, hi) = at(&self.root, &corner, 0).map_err(|(axis, known)| Beyond {
                axis: self.axes[axis].0.clone(),
                dim: self.axes[axis].1,
                wanted: ranges[axis],
                known,
            })?;
            least = least.min(lo);
            most = most.max(hi);
            // The next corner, as an odometer.
            let mut axis = 0;
            while axis < index.len() {
                index[axis] += 1;
                if index[axis] < marks[axis].len() {
                    break;
                }
                index[axis] = 0;
                axis += 1;
            }
            if axis == index.len() {
                break;
            }
        }
        Ok(Quantity::interval(self.dim, least, most).expect("the lowest is below the highest"))
    }

    /// Every point of `axis` strictly inside `lo..hi`, added to `out`.
    fn coordinates(&self, axis: usize, out: &mut Vec<f64>, lo: f64, hi: f64) {
        fn walk(node: &Node, depth: usize, axis: usize, lo: f64, hi: f64, out: &mut Vec<f64>) {
            if let Node::Axis(groups) = node {
                for (x, sub) in groups {
                    if depth == axis {
                        if *x > lo && *x < hi && !out.contains(x) {
                            out.push(*x);
                        }
                    } else {
                        walk(sub, depth + 1, axis, lo, hi, out);
                    }
                }
            }
        }
        walk(&self.root, 0, axis, lo, hi, out);
    }
}

/// Lexicographic order of two points of the axes.
fn compare(a: &[f64], b: &[f64]) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.total_cmp(y))
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// The tree of sorted points, each different from the others.
fn build(points: &[(&[f64], (f64, f64))]) -> Node {
    if points[0].0.is_empty() {
        return Node::Value(points[0].1.0, points[0].1.1);
    }
    let mut groups: Vec<(f64, Node)> = Vec::new();
    let mut start = 0;
    while start < points.len() {
        let x = points[start].0[0];
        let end = start
            + points[start..]
                .iter()
                .take_while(|(at, _)| at[0] == x)
                .count();
        let rest: Vec<(&[f64], (f64, f64))> = points[start..end]
            .iter()
            .map(|(at, v)| (&at[1..], *v))
            .collect();
        groups.push((x, build(&rest)));
        start = end;
    }
    Node::Axis(groups)
}

/// The value at one point of every axis from `depth` on, or the axis it is past and the range
/// the points cover there.
fn at(node: &Node, point: &[f64], depth: usize) -> Result<(f64, f64), (usize, (f64, f64))> {
    let groups = match node {
        Node::Value(lo, hi) => return Ok((*lo, *hi)),
        Node::Axis(groups) => groups,
    };
    let x = point[depth];
    let (first, last) = (groups[0].0, groups[groups.len() - 1].0);
    if x < first || x > last {
        return Err((depth, (first, last)));
    }
    let i = groups.partition_point(|(g, _)| *g < x);
    let (x1, upper) = &groups[i];
    if *x1 == x {
        return at(upper, point, depth + 1);
    }
    let (x0, lower) = &groups[i - 1];
    let (a, b) = (at(lower, point, depth + 1)?, at(upper, point, depth + 1)?);
    let t = (x - x0) / (x1 - x0);
    Ok((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIODE: &str = "temperature,current,forward_voltage
25degC,1mA,0.30V
25degC,100mA,0.42V
25degC,1A,0.55V
125degC,1mA,0.18V
125degC,1A,0.45V
";

    fn kelvin(c: f64) -> f64 {
        c + 273.15
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_point_of_the_file_is_its_value() {
        let c = Curve::parse("d.csv", DIODE).unwrap();
        assert_eq!(c.column, "forward_voltage");
        assert_eq!(c.axes.len(), 2);
        let q = c.over(&[(kelvin(25.0), kelvin(25.0)), (0.1, 0.1)]).unwrap();
        assert!(close(q.lo(), 0.42) && close(q.hi(), 0.42), "{q}");
    }

    #[test]
    fn between_points_the_value_is_a_straight_line_in_each_axis() {
        let c = Curve::parse("d.csv", DIODE).unwrap();
        // Halfway along the current at 25 C: between 0.42 V at 100 mA and 0.55 V at 1 A.
        let q = c
            .over(&[(kelvin(25.0), kelvin(25.0)), (0.55, 0.55)])
            .unwrap();
        assert!(close(q.lo(), 0.485), "{q}");
        // Halfway in temperature at 1 A: between 0.55 V and 0.45 V. The 125 C line has no
        // 100 mA point, and is read along its own currents.
        let q = c.over(&[(kelvin(75.0), kelvin(75.0)), (1.0, 1.0)]).unwrap();
        assert!(close(q.lo(), 0.50), "{q}");
    }

    #[test]
    fn over_ranges_the_value_is_the_lowest_to_the_highest_the_lines_reach() {
        let c = Curve::parse("d.csv", DIODE).unwrap();
        let q = c
            .over(&[(kelvin(25.0), kelvin(125.0)), (0.001, 1.0)])
            .unwrap();
        assert!(close(q.lo(), 0.18) && close(q.hi(), 0.55), "{q}");
    }

    #[test]
    fn a_range_past_the_points_has_no_value() {
        let c = Curve::parse("d.csv", DIODE).unwrap();
        let e = c
            .over(&[(kelvin(25.0), kelvin(25.0)), (0.0, 0.5)])
            .unwrap_err();
        assert_eq!(e.axis, "current");
        assert_eq!(e.known, (0.001, 1.0));
        let e = c
            .over(&[(kelvin(-40.0), kelvin(25.0)), (0.01, 0.5)])
            .unwrap_err();
        assert_eq!(e.axis, "temperature");
        assert!(e.to_string().contains("the curve covers"), "{e}");
    }

    #[test]
    fn a_value_may_be_a_range() {
        let c = Curve::parse("d.csv", "current,forward_voltage\n0A,0V\n1A,0.5V..0.6V\n").unwrap();
        let q = c.over(&[(0.5, 0.5)]).unwrap();
        assert!(close(q.lo(), 0.25) && close(q.hi(), 0.30), "{q}");
    }

    #[test]
    fn a_file_that_cannot_be_read_names_its_line() {
        for (text, why) in [
            ("", "the file is empty"),
            ("forward_voltage\n", "one axis or more"),
            ("current,forward_voltage\n", "no points"),
            ("current,current\n1A,1A\n", "named twice"),
            ("current,forward_voltage\n1A\n", "1 cells"),
            (
                "current,forward_voltage\n1A,0.5V\n2V,0.6V\n",
                "d.csv:3: `current`",
            ),
            ("current,forward_voltage\n1A..2A,0.5V\n", "a range"),
            (
                "current,forward_voltage\n1A,0.5V\n1A,0.6V\n",
                "d.csv:3: the same point as line 2",
            ),
            ("current,forward_voltage\n1A,fast\n", "d.csv:2"),
        ] {
            let e = Curve::parse("d.csv", text).unwrap_err();
            assert!(e.starts_with("d.csv"), "{e}");
            assert!(e.contains(why), "{text:?}: {e}");
        }
    }
}
