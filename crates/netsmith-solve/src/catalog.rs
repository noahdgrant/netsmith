//! Lists of real parts a part's fields are matched against.
//!
//! A catalog is ordered, and the solver takes the first candidate that works, so the
//! order is the selection policy: sort by price for "cheapest", by stock for "in stock",
//! by value for "smallest". Catalogs are data files, one CSV per catalog (ADR 0022, and
//! [`crate::table`] for the format). The sample catalogs are small and sorted by value, for
//! the solver's own tests; [`Catalogs::jlcpcb`] holds real parts that were checked against
//! JLCPCB and LCSC; a project adds its own with [`Catalogs::add_file`].

use std::collections::BTreeMap;
use std::sync::Arc;

use netsmith_check::FieldKind;
use netsmith_quantity::Quantity;

use crate::generic::{Generic, Sense, Series};
use crate::table::Table;

/// What a source of stock knows about a part (ADR 0047). The solver never asks: only the
/// step that resolves a generic part to an orderable one does, so that a part with no stock
/// is passed over there and not silently dropped from the catalog.
pub trait Availability: std::fmt::Debug + Send + Sync {
    /// Pieces in stock for the stock that matters to the project, or `None` if nobody
    /// knows. `None` is not 0: an unknown stock never excludes a part.
    fn stock(&self, lcsc: &str) -> Option<u64>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Quantity(Quantity),
    Text(String),
    /// A value that depends on another field of the same part (see [`Derating`]).
    Derated(Derating),
    /// A value a generic part (ADR 0047) cannot know and the design supplies: the solver
    /// reads it from the design when it picks the part. Only generic parts have it.
    Open(Sense),
}

/// A capacitor's effective capacitance: the marked value with its tolerance, and the fraction
/// of it that is left at each bias voltage.
///
/// This is the usual datasheet formula, `C_eff = C_bias x (1 - tolerance)` at the low end, as
/// Analog Devices states it: the fraction from the curve multiplies the bottom of the marked
/// range for a minimum and the top for a maximum. Temperature is not part of it: the curves
/// are typical data at 25 C (ADR 0013).
///
/// The curve is a list of `(volts, fraction)` points with rising volts. Between two points
/// the fraction is interpolated in a straight line; over a range of bias the lowest and
/// highest fractions are the ones at the ends of the range or at a point inside it. An empty
/// curve means nobody has published data for the part, and then no effective capacitance can
/// be given at any voltage. So does a bias past the last point: the part is never assumed to
/// behave beyond what was measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Derating {
    /// The marked capacitance with its tolerance.
    pub marked: Quantity,
    /// The field of the same part that holds the DC voltage across it (`bias`).
    pub against: String,
    pub curve: Vec<(f64, f64)>,
}

/// Why a derated value could not be worked out.
#[derive(Debug, Clone, PartialEq)]
pub enum DeratingError {
    /// The catalog has no published curve for this part.
    NoData,
    /// The bias goes past the last voltage the curve covers.
    BeyondData { wanted: f64, known_to: f64 },
}

impl Derating {
    /// The capacitance the part keeps over a range of bias, in the worst case: the lowest
    /// capacitance it can have at the bottom, the highest at the top. The sign of the bias
    /// does not matter.
    pub fn at(&self, bias: &Quantity) -> Result<Quantity, DeratingError> {
        let Some(&(known_to, _)) = self.curve.last() else {
            return Err(DeratingError::NoData);
        };
        let (a, b) = (bias.lo(), bias.hi());
        let low = if a <= 0.0 && b >= 0.0 {
            0.0
        } else {
            a.abs().min(b.abs())
        };
        let high = a.abs().max(b.abs());
        if high > known_to {
            return Err(DeratingError::BeyondData {
                wanted: high,
                known_to,
            });
        }
        // A piecewise-linear curve is at its extremes at its ends or its points.
        let inside = self.curve.iter().filter(|(v, _)| *v > low && *v < high);
        let fractions: Vec<f64> = [self.fraction(low), self.fraction(high)]
            .into_iter()
            .chain(inside.map(|(_, f)| *f))
            .collect();
        let least = fractions.iter().copied().fold(f64::INFINITY, f64::min);
        let most = fractions.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Ok(Quantity::interval(
            self.marked.dim(),
            self.marked.lo() * least,
            self.marked.hi() * most,
        )
        .expect("a fraction of a capacitance is a capacitance"))
    }

    /// The fraction left at one bias, which is inside the curve.
    fn fraction(&self, volts: f64) -> f64 {
        let i = self.curve.partition_point(|(v, _)| *v < volts);
        match (i.checked_sub(1).map(|j| self.curve[j]), self.curve.get(i)) {
            (_, None) => self.curve[self.curve.len() - 1].1,
            (None, Some(&(_, f))) => f,
            (Some((v0, f0)), Some(&(v1, f1))) => f0 + (f1 - f0) * (volts - v0) / (v1 - v0),
        }
    }
}

/// Whether JLCPCB stocks a part in its Basic library (no feeder fee), its Extended library
/// (a feeder fee for each different part), or its Preferred Extended parts (Extended, with
/// no fee).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    Basic,
    Extended,
    Preferred,
}

impl std::fmt::Display for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Basic => "Basic",
            Self::Extended => "Extended",
            Self::Preferred => "Preferred",
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Candidate {
    /// What a person calls it in a message: `300Ω 1% 0402`.
    pub name: String,
    /// What goes in a schematic's value field: `300`.
    pub label: String,
    /// One entry per catalog column, keyed by the field's path inside the part.
    pub columns: Vec<(String, Value)>,
    /// The supplier's part number (LCSC `C25104`), if this is a real part that can be bought.
    pub lcsc: Option<String>,
    /// The manufacturer's part number.
    pub mpn: Option<String>,
    pub library: Option<Library>,
    /// Who assembles the part when the design does not say: `fab`, `hand` or `none`
    /// (ADR 0024). The row's `assembly` cell, if it has one.
    pub assembly: Option<String>,
    /// Set for a generic part (ADR 0047): it has no supplier number, and a later step
    /// resolves it to a row of the catalog.
    pub generic: Option<Generic>,
}

impl Candidate {
    pub fn column(&self, key: &str) -> Option<&Value> {
        self.columns.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub name: String,
    pub candidates: Vec<Candidate>,
}

/// The catalogs a build can pick from: typed ones that code inserted, and the tables read
/// from catalog files, which are typed when a part says what its columns are.
#[derive(Debug, Clone, Default)]
pub struct Catalogs {
    map: BTreeMap<String, Catalog>,
    tables: BTreeMap<String, Table>,
    default_assembly: Option<String>,
    strict_guidelines: bool,
    constraints_warn: bool,
    series: Option<Series>,
    availability: Option<Arc<dyn Availability>>,
}

impl Catalogs {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The project's default for who assembles a part that nothing else speaks for:
    /// `fab`, `hand` or `none` (the manifest's `assembly`, ADR 0028). The sourcing policy of
    /// a project travels with its parts data, so every command that picks parts lowers with
    /// it. The caller has checked the word.
    pub fn with_default_assembly(mut self, word: Option<&str>) -> Self {
        self.default_assembly = word.map(str::to_string);
        self
    }

    pub fn default_assembly(&self) -> Option<&str> {
        self.default_assembly.as_deref()
    }

    /// Whether a guideline finding (ADR 0036) stops the build, as the manifest's
    /// `guidelines = "error"` or the command line's `--strict` say. Like the default
    /// assembly it is a policy of the project that every command applies, so it travels
    /// here.
    pub fn with_strict_guidelines(mut self, strict: bool) -> Self {
        self.strict_guidelines = strict;
        self
    }

    pub fn strict_guidelines(&self) -> bool {
        self.strict_guidelines
    }

    /// Whether a requirement on where a part goes that the placed board does not meet is
    /// only a warning (the manifest's `constraints = "warn"` or the command line's
    /// `--constraints warn`, ADR 0045). Otherwise it is an error that stops `fab`. A policy
    /// of the project, so it travels here like the guidelines'.
    pub fn with_constraints_warn(mut self, warn: bool) -> Self {
        self.constraints_warn = warn;
        self
    }

    pub fn constraints_warn(&self) -> bool {
        self.constraints_warn
    }

    /// Offer generic parts of this E-series after the rows of a catalog that has them
    /// (ADR 0047). With `None`, the default, only rows are offered.
    pub fn with_series(mut self, series: Option<Series>) -> Self {
        self.series = series;
        self
    }

    pub fn series(&self) -> Option<Series> {
        self.series
    }

    /// Where the step that resolves generic parts asks about stock (ADR 0047). It travels
    /// with the catalogs, like the project's other policies.
    pub fn with_availability(mut self, availability: Arc<dyn Availability>) -> Self {
        self.availability = Some(availability);
        self
    }

    pub fn availability(&self) -> Option<&Arc<dyn Availability>> {
        self.availability.as_ref()
    }

    /// A catalog made in code. It replaces any file of the same name.
    pub fn insert(&mut self, catalog: Catalog) {
        self.tables.remove(&catalog.name);
        self.map.insert(catalog.name.clone(), catalog);
    }

    /// A catalog made in code, by name. The catalogs read from files are not here, because
    /// a cell cannot be read before the part says what its column holds: see
    /// [`Self::resolve`].
    pub fn get(&self, name: &str) -> Option<&Catalog> {
        self.map.get(name)
    }

    /// Whether a catalog of this name exists, made in code or read from a file.
    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name) || self.tables.contains_key(name)
    }

    /// Read a catalog file (ADR 0022). `name` is the catalog's, `file` is what a message
    /// calls the file. A catalog of that name that is already here gets these rows after
    /// its own, unless one of them is a part it has already: that is an error naming both.
    pub fn add_file(&mut self, name: &str, file: &str, text: &str) -> Result<(), String> {
        let table = Table::parse(file, text)?;
        if let Some(code) = self.map.get(name)
            && !code.candidates.is_empty()
        {
            return Err(format!(
                "{file}: the catalog `{name}` was built in code and cannot take rows from a file"
            ));
        }
        match self.tables.get_mut(name) {
            Some(existing) => existing.append(table),
            None => {
                self.tables.insert(name.to_string(), table);
                Ok(())
            }
        }
    }

    /// The catalog `name` for a part whose `catalog name { ... }` lists `columns`, with every
    /// cell of a file read as its column's kind. `None` if there is no such catalog. An
    /// error names the file and line of the cell. If the project states an E-series, the
    /// generic parts of that series come after the rows (ADR 0047), so a value with a row is
    /// picked as before and only a value without one is new.
    pub fn resolve(
        &self,
        name: &str,
        columns: &[(String, FieldKind)],
    ) -> Option<Result<Catalog, String>> {
        let mut found = self.identity(name, columns)?;
        if let (Ok(catalog), Some(series)) = (&mut found, self.series)
            && let Some(generic) = crate::generic::candidates(series, name, columns)
        {
            catalog.candidates.extend(generic);
        }
        Some(found)
    }

    /// [`Self::resolve`], without generic parts: only the rows, which name real parts.
    pub fn identity(
        &self,
        name: &str,
        columns: &[(String, FieldKind)],
    ) -> Option<Result<Catalog, String>> {
        if let Some(catalog) = self.map.get(name) {
            return Some(Ok(catalog.clone()));
        }
        let table = self.tables.get(name)?;
        Some(table.candidates(name, columns).map(|candidates| Catalog {
            name: name.to_string(),
            candidates,
        }))
    }

    /// What the catalog files say about an LCSC number, the standard ones and a project's
    /// own alike. `None` for a part that no file lists.
    pub fn supplier(&self, lcsc: &str) -> Option<crate::table::Supplier> {
        self.tables.values().find_map(|t| t.supplier(lcsc))
    }

    /// Small catalogs for resistors, capacitors and linear regulators, from
    /// `data/sample/`. There is none for LEDs, so an LED stays unresolved. The parts in
    /// them are made up for the solver's tests and have no supplier numbers.
    pub fn sample() -> Self {
        crate::jlcpcb::embedded(crate::jlcpcb::SAMPLE)
    }

    /// Real parts for JLCPCB assembly, from `data/jlcpcb/catalog/`. See [`crate::jlcpcb`]
    /// for the files and `data/jlcpcb/README.md` for where each row comes from.
    pub fn jlcpcb() -> Self {
        crate::jlcpcb::embedded(crate::jlcpcb::JLCPCB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quantity(value: f64, unit: &str) -> Quantity {
        Quantity::literal(value, Some(unit)).unwrap().quantity
    }

    fn with_tolerance(value: Quantity, percent: f64) -> Quantity {
        Quantity::tolerance(&value, &quantity(percent, "%"), true).unwrap()
    }

    /// 10 uF +/- 20% that keeps 100% at 0 V, 60% at 4 V and 20% at 10 V.
    fn derated() -> Derating {
        Derating {
            marked: with_tolerance(quantity(10e-6, "F"), 20.0),
            against: "bias".into(),
            curve: vec![(0.0, 1.0), (4.0, 0.6), (10.0, 0.2)],
        }
    }

    fn volts(lo: f64, hi: f64) -> Quantity {
        Quantity::range(&quantity(lo, "V"), &quantity(hi, "V")).unwrap()
    }

    fn near(q: Quantity, lo: f64, hi: f64) -> bool {
        (q.lo() - lo).abs() < 1e-12 && (q.hi() - hi).abs() < 1e-12
    }

    #[test]
    fn derating_at_a_point_is_the_tolerance_times_the_fraction_left() {
        // Halfway from 4 V (0.6) to 10 V (0.2) is 7 V, where 0.4 is left.
        let q = derated().at(&volts(7.0, 7.0)).unwrap();
        assert!(near(q, 8e-6 * 0.4, 12e-6 * 0.4), "{q}");
    }

    #[test]
    fn derating_over_a_range_takes_the_worst_end_of_each_side() {
        // 2..6 V: 0.8 at 2 V, 0.6 at the point 4 V and 0.6 - 0.4 * 2 / 6 at 6 V.
        let q = derated().at(&volts(2.0, 6.0)).unwrap();
        let least = 0.6 - 0.4 * 2.0 / 6.0;
        assert!(near(q, 8e-6 * least, 12e-6 * 0.8), "{q}");
    }

    #[test]
    fn derating_looks_at_a_point_inside_the_range_that_is_not_monotone() {
        let bump = Derating {
            curve: vec![(0.0, 1.0), (2.0, 0.5), (4.0, 0.9), (6.0, 0.9)],
            ..derated()
        };
        // Neither 1 V (0.75) nor 5 V (0.9) is the least: the point at 2 V is (0.5).
        let q = bump.at(&volts(1.0, 5.0)).unwrap();
        assert!(near(q, 8e-6 * 0.5, 12e-6 * 0.9), "{q}");
    }

    #[test]
    fn the_sign_of_the_bias_does_not_matter_and_zero_keeps_everything() {
        let d = derated();
        assert_eq!(d.at(&volts(-7.0, -7.0)), d.at(&volts(7.0, 7.0)));
        let q = d.at(&volts(-2.0, 2.0)).unwrap();
        assert!(near(q, 8e-6 * 0.8, 12e-6), "{q}");
    }

    #[test]
    fn derating_refuses_a_bias_past_the_curve_and_a_part_with_no_curve() {
        assert_eq!(
            derated().at(&volts(9.0, 10.5)),
            Err(DeratingError::BeyondData {
                wanted: 10.5,
                known_to: 10.0
            })
        );
        let none = Derating {
            curve: Vec::new(),
            ..derated()
        };
        assert_eq!(none.at(&volts(1.0, 1.0)), Err(DeratingError::NoData));
    }
}
