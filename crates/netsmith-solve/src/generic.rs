//! Generic passives (ADR 0047): a part the solver chooses by value alone, and
//! the step that later resolves it to a row of a catalog.
//!
//! A generic resistor is a value of an E-series (E12, E24 or E96) with a tolerance, a package
//! and a power rating, and no supplier. The solver sees it as one more candidate, so it is
//! never stuck on a value that no row of the catalog has. [`resolve`] runs afterwards, when
//! the bill of materials or the fab package is made: it finds the row of the catalog that is
//! at least as good (Basic first), or says which rows are nearest.
//!
//! Capacitors, inductors and LEDs are generic too (ADR 0047), with one
//! difference: a rating a generic part cannot know is left **open**. The solver fills it in
//! from the design when the part is picked (what the design states, or the least a relation
//! demands) and the generic part remembers it as a [`Need`], so the row that [`resolve`]
//! finds has to meet it. A generic capacitor has no bias curve, so it resolves only to a row
//! that has a recorded one (owner decision, 2026-11-03).
//!
//! **What the decision says** (ADR 0047). The solver may pick any value of the project's series,
//! so it is never stuck on a value no known supplier row stocks; the series is the manifest's
//! `series` key (E24 by default), a policy of the project like `assembly`, and not a statement
//! of the language (owner, 2026-11-04). The power rating of a package is the usual one (0402
//! 62.5 mW, 0603 100 mW, 0805 125 mW): a package's rating and not a supplier's data. The generic
//! parts come after the catalog rows, so one is reached only when no row serves the design as
//! stated; a generic capacitor, inductor or LED therefore mostly ends *unresolved*, listed by
//! name with the nearest rows and the result of the checks with each. That is what the owner
//! asked for ("unresolved reported by name"), and it is the useful part.
//!
//! **Resolution order.** For a generic part, a row of the same catalog with the same package, a
//! nominal equal to the generic value, a tolerance no wider and a power rating no lower (the
//! row must be at least as good as what the solver checked). Among those, the preferred-parts
//! order: JLCPCB Basic, then Preferred, then Extended, and inside a class the file's order, so
//! a project's own rows are tried last. A row a provider says has stock 0 is passed over and
//! noted; unknown stock does not exclude. The row's `lcsc`, `mpn` and `library` go to the part;
//! its value label is unchanged. A part no row fits is unresolved, `fab` refuses by name, and
//! the message gives up to three nearest rows (in the same package first, then others) and ends
//! with "Change the value in the design and build again": the build then checks every relation
//! again with it.
//!
//! **What the generic capacitor, inductor and LED are** (choices of 2026-11-04, issue #31,
//! accepted as built by the owner, 2026-11-11; the owner's decision of 2026-11-03 is applied as
//! given). A capacitor is a value of the series (1 pF to 100 uF), a
//! tolerance (20%, 10% or 5%, widest tried first, so the weakest claim is the easiest to resolve)
//! and a 0402, 0603 or 0805 package; an inductor is 1 nH to 1 mH, 30%, 20% or 10%, in 0402,
//! 0603, 0805 or 1210; an LED is a package with colour and forward voltage open. The saturation
//! current and the DC resistance are open. A capacitor's `effective_capacitance` is a derated
//! value with an empty curve, so the solver does not use it where the design states or relates
//! `.effective_capacitance`, and says "generic parts have no recorded bias curve"; resolution
//! accepts a capacitor row only if its cell has a curve (`Miss::NoCurve`).
//!
//! **Assumptions**, word for word from ADR 0047 (accepted as built by the owner, 2026-11-11):
//! 1. "The series of a capacitor or an inductor is the project's `series`, but E96 gives E24: E96
//!    is a resistor series. The alternative is a fixed E12, or a series key for each kind."
//! 2. "The packages, tolerances and value ranges above. They are the packages `std/kicad.toml`
//!    maps, the tolerance classes M, K and J (IEC 60062) for a capacitor, and the ranges the
//!    catalogs sit in. They are names and classes, not a supplier's data. A capacitor's voltage
//!    and an inductor's ratings are not on a ladder, because a ladder of ratings would be data
//!    nobody recorded."
//! 3. "`Stated` for an LED: the generic LED is used only if the design states the forward
//!    voltage. The alternative, `Nothing` for it, would let the forward voltage be unknown and
//!    every check that depends on it (the resistor that sets the LED's current) would go
//!    unevaluated."
//! 4. "Leaving a rating unknown when nothing bounds it (`Nothing`) instead of refusing the
//!    generic part. An unbounded rating is reported as not evaluated, as any unknown value is.
//!    The need is read when the part is picked: a relation whose other side has no value yet at
//!    that moment (a current that later parts settle) does not give a need, and the rating is
//!    then left unknown."
//! 5. "The cap of 30 solves, and that a swapped-in alternative is judged with the part's stated
//!    numbers left out but its text statements (package, colour) kept."
//!
//! **Gaps in the catalogs** (reported, nothing added; a column is added only with a recorded
//! source, `data/jlcpcb/README.md`, and not planned until a board needs it): an LED has no maximum forward
//! current column, so `Led` states no limit on `.current`, and a forward voltage is the
//! datasheet's range at a test current (5 or 20 mA) that is not a column; an inductor has no
//! temperature-rise (rms) current column, only the saturation current; a capacitor has no
//! dielectric or temperature-coefficient column, so a generic capacitor cannot state C0G or
//! X7R, and a bias curve is a typical value at 25 C (ADR 0013). A generic part that needs any of
//! these cannot be held to it.

use netsmith_check::FieldKind;
use netsmith_quantity::{Dimension, Quantity};

use crate::catalog::{Candidate, Catalogs, Derating, Library, Value};
use crate::standard::{Fill, Standard};

/// An E-series: the values of one decade that a tolerance class is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Series {
    E12,
    E24,
    E96,
}

impl Series {
    /// `E12`, `E24` or `E96`, in any case.
    pub fn parse(word: &str) -> Option<Series> {
        match word.trim().to_ascii_uppercase().as_str() {
            "E12" => Some(Series::E12),
            "E24" => Some(Series::E24),
            "E96" => Some(Series::E96),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Series::E12 => "E12",
            Series::E24 => "E24",
            Series::E96 => "E96",
        }
    }

    /// The values of one decade, from 1.0 up to but not including 10, as the standard
    /// writes them (IEC 60063).
    fn decade(self) -> &'static [&'static str] {
        match self {
            Series::E12 => &[
                "1.0", "1.2", "1.5", "1.8", "2.2", "2.7", "3.3", "3.9", "4.7", "5.6", "6.8", "8.2",
            ],
            Series::E24 => &[
                "1.0", "1.1", "1.2", "1.3", "1.5", "1.6", "1.8", "2.0", "2.2", "2.4", "2.7", "3.0",
                "3.3", "3.6", "3.9", "4.3", "4.7", "5.1", "5.6", "6.2", "6.8", "7.5", "8.2", "9.1",
            ],
            Series::E96 => &[
                "1.00", "1.02", "1.05", "1.07", "1.10", "1.13", "1.15", "1.18", "1.21", "1.24",
                "1.27", "1.30", "1.33", "1.37", "1.40", "1.43", "1.47", "1.50", "1.54", "1.58",
                "1.62", "1.65", "1.69", "1.74", "1.78", "1.82", "1.87", "1.91", "1.96", "2.00",
                "2.05", "2.10", "2.15", "2.21", "2.26", "2.32", "2.37", "2.43", "2.49", "2.55",
                "2.61", "2.67", "2.74", "2.80", "2.87", "2.94", "3.01", "3.09", "3.16", "3.24",
                "3.32", "3.40", "3.48", "3.57", "3.65", "3.74", "3.83", "3.92", "4.02", "4.12",
                "4.22", "4.32", "4.42", "4.53", "4.64", "4.75", "4.87", "4.99", "5.11", "5.23",
                "5.36", "5.49", "5.62", "5.76", "5.90", "6.04", "6.19", "6.34", "6.49", "6.65",
                "6.81", "6.98", "7.15", "7.32", "7.50", "7.68", "7.87", "8.06", "8.25", "8.45",
                "8.66", "8.87", "9.09", "9.31", "9.53", "9.76",
            ],
        }
    }

    /// Every value of the series from 1 ohm up to 9.76 Mohm, rising. A value is the nearest
    /// double to its decimal digits, so `3.3` times 100 is exactly 330.
    pub fn resistances(self) -> Vec<f64> {
        self.values(0..=6)
    }

    /// The values of the decades `10^exp` for each `exp` given, rising.
    pub fn values(self, decades: std::ops::RangeInclusive<i32>) -> Vec<f64> {
        decades
            .flat_map(|exp| {
                self.decade().iter().map(move |m| {
                    format!("{m}e{exp}")
                        .parse::<f64>()
                        .expect("a table entry is a number")
                })
            })
            .collect()
    }

    /// The series a part whose definition offers `offered` (its `#[series(...)]`, ADR 0123) is
    /// given values of, when the project states this one: this one if it is offered, else the
    /// finest offered series coarser than it (a project on E96 gives a capacitor that offers
    /// E12 and E24 its E24 values), else none.
    pub fn within(self, offered: &[Series]) -> Option<Series> {
        if offered.contains(&self) {
            return Some(self);
        }
        offered
            .iter()
            .copied()
            .filter(|s| s.rank() < self.rank())
            .max_by_key(|s| s.rank())
    }

    /// Coarsest first.
    fn rank(self) -> u8 {
        match self {
            Series::E12 => 0,
            Series::E24 => 1,
            Series::E96 => 2,
        }
    }

    /// The tolerance the series is made for (IEC 60063): E12 10%, E24 5%, E96 1%. A part of a
    /// project's own catalog that the solver makes of a series has it.
    pub fn tolerance_percent(self) -> f64 {
        match self {
            Series::E12 => 10.0,
            Series::E24 => 5.0,
            Series::E96 => 1.0,
        }
    }
}

/// The packages a generic resistor can be, in the order they are tried, with the power each
/// is rated for. These are the common ratings of the package sizes, not a supplier's data.
const PACKAGES: [(&str, f64); 3] = [("0402", 0.0625), ("0603", 0.1), ("0805", 0.125)];

/// The tolerance of a generic resistor, in percent. E24 and E96 values are made at 1%.
const TOLERANCE_PERCENT: f64 = 1.0;

/// The packages a generic capacitor can be: the sizes the capacitor mapping has
/// (`std/kicad.toml`). A package is a name, not a rating.
const CAPACITOR_PACKAGES: [&str; 3] = ["0402", "0603", "0805"];

/// The tolerances a generic capacitor can have, widest first: the standard classes M, K and
/// J of IEC 60062. The widest is the weakest claim, so it is tried first and is the easiest
/// to resolve.
const CAPACITOR_TOLERANCES: [f64; 3] = [20.0, 10.0, 5.0];

/// The packages a generic inductor can be (`std/kicad.toml`).
const INDUCTOR_PACKAGES: [&str; 4] = ["0402", "0603", "0805", "1210"];

/// The tolerances a generic inductor can have, widest first.
const INDUCTOR_TOLERANCES: [f64; 3] = [30.0, 20.0, 10.0];

/// The packages a generic LED can be (`std/kicad.toml`).
const LED_PACKAGES: [&str; 3] = ["0402", "0603", "0805"];

/// How the solver reads a column that a generic part leaves open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sense {
    /// Only what the design states for the field: a colour, a forward voltage. A value
    /// that other values are worked out from is never guessed, so a generic part that
    /// finds the design asking for the field without stating it is not used.
    Stated,
    /// A rating where more is better (a voltage or current rating): the least that any
    /// relation of the design demands of it.
    AtLeast,
    /// A rating where less is better (a winding's resistance): the most that any relation
    /// of the design allows.
    AtMost,
}

/// What the design asks of a column a generic part left open, as found when the part was
/// picked. The row that stands for the part has to meet it.
#[derive(Debug, Clone, PartialEq)]
pub enum Need {
    /// Nothing: the design neither states the field nor demands anything of it. The solver
    /// then did not check it, and the check that mentions it is listed as not evaluated.
    Nothing,
    /// The design states this text (`RED`).
    Text(String),
    /// The design states a range, in the field's base unit. The row's value must lie in it.
    Within { lo: f64, hi: f64 },
    /// A relation demands at least this much.
    AtLeast(f64),
    /// A relation allows at most this much.
    AtMost(f64),
}

impl Need {
    fn is_nothing(&self) -> bool {
        matches!(self, Need::Nothing)
    }

    fn met_by(&self, value: Option<&Value>) -> bool {
        let quantity = match value {
            Some(Value::Quantity(q)) => Some(q),
            _ => None,
        };
        match self {
            Need::Nothing => true,
            Need::Text(t) => matches!(value, Some(Value::Text(v)) if v == t),
            Need::Within { lo, hi } => quantity.is_some_and(|q| {
                q.lo() >= lo - lo.abs() * 1e-9 - 1e-15 && q.hi() <= hi + hi.abs() * 1e-9 + 1e-15
            }),
            Need::AtLeast(x) => quantity.is_some_and(|q| q.lo() >= x - x.abs() * 1e-9),
            Need::AtMost(x) => quantity.is_some_and(|q| q.hi() <= x + x.abs() * 1e-9),
        }
    }

    /// How far a row's value is from meeting the need: `0` if it meets it, up to `1` for a
    /// row that has nothing to compare or is off by its whole size. Relative, so that the
    /// shortfalls of different columns can be added.
    fn shortfall(&self, value: Option<&Value>) -> f64 {
        let quantity = match value {
            Some(Value::Quantity(q)) => Some(q),
            _ => None,
        };
        let over = |amount: f64, scale: f64| (amount / scale.abs().max(f64::MIN_POSITIVE)).min(1.0);
        match self {
            Need::Nothing => 0.0,
            Need::Text(_) => {
                if self.met_by(value) {
                    0.0
                } else {
                    1.0
                }
            }
            Need::Within { lo, hi } => quantity.map_or(1.0, |q| {
                over((lo - q.lo()).max(0.0), *lo) + over((q.hi() - hi).max(0.0), *hi)
            }),
            Need::AtLeast(x) => quantity.map_or(1.0, |q| over((x - q.lo()).max(0.0), *x)),
            Need::AtMost(x) => quantity.map_or(1.0, |q| over((q.hi() - x).max(0.0), q.hi())),
        }
        .min(1.0)
    }

    /// `at least 10 V`, or `` for nothing.
    fn describe(&self, unit: &str) -> String {
        match self {
            Need::Nothing => String::new(),
            Need::Text(t) => t.clone(),
            Need::Within { lo, hi } => format!("{}..{}", si(*lo, unit), si(*hi, unit)),
            Need::AtLeast(x) => format!("at least {}", si(*x, unit)),
            Need::AtMost(x) => format!("at most {}", si(*x, unit)),
        }
    }
}

/// What a generic part is, by kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Spec {
    /// A value of an E-series with a tolerance and the package's usual power rating.
    Resistor {
        series: Series,
        /// In ohms.
        nominal: f64,
        tolerance_percent: f64,
        /// In watts.
        max_power: f64,
    },
    /// A ceramic capacitor's marked value. Its effective capacitance is unknown: a generic
    /// capacitor has no bias curve, and resolves only to a row that has one.
    Capacitor {
        series: Series,
        /// In farads.
        nominal: f64,
        tolerance_percent: f64,
        /// The rated voltage the design demands.
        max_voltage: Need,
    },
    Inductor {
        series: Series,
        /// In henries.
        nominal: f64,
        tolerance_percent: f64,
        saturation_current: Need,
        dc_resistance: Need,
    },
    Led {
        color: Need,
        forward_voltage: Need,
    },
    /// A value of an E-series in a catalog of a project's own generic that declares
    /// `#[series(...)]` (ADR 0123): the first column of its `catalog` statement is the value,
    /// with the series' own tolerance, and every other column is what the design states.
    Series {
        series: Series,
        /// The column the value is in, the first of the catalog statement.
        column: String,
        dim: Dimension,
        /// In the base unit of `dim`.
        nominal: f64,
        tolerance_percent: f64,
        /// The catalog statement's columns, in its order, with what each holds.
        columns: Vec<(String, FieldKind)>,
        /// What the design states for each other column, keyed by column.
        needs: Vec<(String, Need)>,
    },
}

/// What a generic part asks of the part that stands for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Generic {
    /// The catalog its row must be in (`resistor`).
    pub catalog: String,
    pub package: String,
    pub spec: Spec,
}

impl Generic {
    /// The value, in the base unit of its dimension. An LED has none.
    pub fn nominal(&self) -> Option<f64> {
        match &self.spec {
            Spec::Resistor { nominal, .. }
            | Spec::Capacitor { nominal, .. }
            | Spec::Inductor { nominal, .. }
            | Spec::Series { nominal, .. } => Some(*nominal),
            Spec::Led { .. } => None,
        }
    }

    /// The series the value is from. An LED has none.
    pub fn series(&self) -> Option<Series> {
        match &self.spec {
            Spec::Resistor { series, .. }
            | Spec::Capacitor { series, .. }
            | Spec::Inductor { series, .. }
            | Spec::Series { series, .. } => Some(*series),
            Spec::Led { .. } => None,
        }
    }

    /// The column that holds the value, for a kind that has one.
    fn value_column(&self) -> Option<&str> {
        match &self.spec {
            Spec::Series { column, .. } => Some(column),
            spec => Standard::of_spec(spec).and_then(Standard::marked),
        }
    }

    /// The columns a row of its catalog is read with.
    fn kinds(&self) -> Vec<(String, FieldKind)> {
        match &self.spec {
            Spec::Series { columns, .. } => columns.clone(),
            spec => Standard::of_spec(spec).map_or_else(Vec::new, Standard::kinds),
        }
    }

    /// What a message calls one of its parts.
    fn word(&self) -> &'static str {
        Standard::of_spec(&self.spec).map_or("part", |s| s.word)
    }
    /// `340 ohm 1% 0402 62.5 mW (E24)`, `100 nF 10% 0402 at least 10 V (E24)`,
    /// `Red LED 0603 forward voltage 1.8 V..2.2 V`.
    pub fn describe(&self) -> String {
        let with = |parts: Vec<String>| {
            parts
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match &self.spec {
            Spec::Resistor {
                series,
                nominal,
                tolerance_percent,
                max_power,
            } => format!(
                "{} ohm {}% {} {} mW ({})",
                number(*nominal),
                number(*tolerance_percent),
                self.package,
                number(max_power * 1000.0),
                series.name()
            ),
            Spec::Capacitor {
                series,
                nominal,
                tolerance_percent,
                max_voltage,
            } => format!(
                "{} {}% {}{} ({}, needs a recorded DC-bias curve)",
                si(*nominal, "F"),
                number(*tolerance_percent),
                self.package,
                rated(max_voltage, "rated voltage", "V"),
                series.name()
            ),
            Spec::Inductor {
                series,
                nominal,
                tolerance_percent,
                saturation_current,
                dc_resistance,
            } => format!(
                "{} {}% {}{}{} ({})",
                si(*nominal, "H"),
                number(*tolerance_percent),
                self.package,
                rated(saturation_current, "saturation current", "A"),
                rated(dc_resistance, "DC resistance", "ohm"),
                series.name()
            ),
            Spec::Series {
                series,
                dim,
                nominal,
                tolerance_percent,
                needs,
                ..
            } => {
                let unit = dim.unit_symbol().unwrap_or("");
                let mut parts = vec![format!(
                    "{} {}% ({})",
                    si(*nominal, unit),
                    number(*tolerance_percent),
                    series.name()
                )];
                parts.extend(
                    needs
                        .iter()
                        .filter(|(_, n)| !n.is_nothing())
                        .map(|(name, n)| format!("{name} {}", n.describe(""))),
                );
                with(parts)
            }
            Spec::Led {
                color,
                forward_voltage,
            } => with(vec![
                format!(
                    "{}LED {}",
                    if let Need::Text(t) = color {
                        format!("{t} ")
                    } else {
                        String::new()
                    },
                    self.package
                ),
                rated(forward_voltage, "forward voltage", "V")
                    .trim_start_matches(", ")
                    .to_string(),
            ]),
        }
    }

    /// The same generic part with the needs the solver read from the design when it picked
    /// the part. `needs` is keyed by catalog column.
    pub fn with_needs(&self, needs: &[(String, Need)]) -> Generic {
        let find = |key: &str| {
            needs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, n)| n.clone())
                .unwrap_or(Need::Nothing)
        };
        let mut out = self.clone();
        match &mut out.spec {
            Spec::Resistor { .. } => {}
            Spec::Series {
                column,
                columns,
                needs: wanted,
                ..
            } => {
                *wanted = columns
                    .iter()
                    .filter(|(name, _)| name != column)
                    .map(|(name, _)| (name.clone(), find(name)))
                    .collect();
            }
            Spec::Capacitor { max_voltage, .. } => *max_voltage = find("max_voltage"),
            Spec::Inductor {
                saturation_current,
                dc_resistance,
                ..
            } => {
                *saturation_current = find("saturation_current");
                *dc_resistance = find("dc_resistance");
            }
            Spec::Led {
                color,
                forward_voltage,
            } => {
                *color = find("color");
                *forward_voltage = find("forward_voltage");
            }
        }
        out
    }
}

/// `, rated voltage at least 10 V`, or nothing if the design asks nothing.
fn rated(need: &Need, what: &str, unit: &str) -> String {
    if need.is_nothing() {
        String::new()
    } else {
        format!(", {what} {}", need.describe(unit))
    }
}

/// A number the way a person writes it: `62.5`, `340`, with no trailing zeros.
fn number(x: f64) -> String {
    let text = format!("{x:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A quantity with an SI prefix: `100 nF`, `2.2 uH`, `7.875 V`.
fn si(x: f64, unit: &str) -> String {
    // A plain number (a thermistor's beta) is written as it is, with no prefix.
    if unit.is_empty() {
        return number(x);
    }
    let (scaled, prefix) = scaled(x);
    format!("{} {prefix}{unit}", number(scaled))
}

fn scaled(x: f64) -> (f64, &'static str) {
    let a = x.abs();
    let table = [
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "u"),
        (1e-9, "n"),
        (1e-12, "p"),
    ];
    for (factor, prefix) in table {
        if a >= factor * (1.0 - 1e-9) {
            return (x / factor, prefix);
        }
    }
    (x / 1e-12, "p")
}

/// The schematic's value field: `330`, `2.2k`, `1M` for a resistance; `100n` for a
/// capacitance; `2.2u` for an inductance.
fn label(x: f64) -> String {
    let (scaled, prefix) = scaled(x);
    format!("{}{prefix}", number(scaled))
}

/// The generic candidates of a catalog, in the order they are tried (smallest package first,
/// then rising value), if the catalog is one that has generic parts and the part lists only
/// columns they can fill. `None` otherwise. [`Standard`] says which catalogs have them.
pub fn candidates(
    series: Option<Series>,
    offered: &[Series],
    catalog: &str,
    columns: &[(String, FieldKind)],
) -> Option<Vec<Candidate>> {
    let project = series?;
    match Standard::of_catalog(catalog) {
        // A standard catalog's parts are made as its entry says, of the series the part's
        // definition offers; an LED has no value, and so no series to offer.
        Some(standard) if standard.marked().is_none() => standard.candidates(project, columns),
        Some(standard) => standard.candidates(project.within(offered)?, columns),
        None => series_values(project.within(offered)?, catalog, columns),
    }
}

/// The decades a project's own catalog is offered values in: 1 p to 9.1 G of its unit, which
/// covers the catalogs of the standard library and leaves the choice to the design's bounds.
const SERIES_DECADES: std::ops::RangeInclusive<i32> = -12..=9;

/// The parts the solver makes for a catalog of a project's own generic that declares
/// `#[series(...)]` (ADR 0123): each value of the series in [`SERIES_DECADES`], in its first
/// column, with the series' tolerance, and every other column left to what the design states.
/// `None` when the first column is not a quantity.
fn series_values(
    series: Series,
    catalog: &str,
    columns: &[(String, FieldKind)],
) -> Option<Vec<Candidate>> {
    let (column, FieldKind::Quantity(dim)) = columns.first()?.clone() else {
        return None;
    };
    let percent = series.tolerance_percent();
    let mut out = Vec::new();
    for nominal in series.values(SERIES_DECADES) {
        let mut cells = vec![(
            column.clone(),
            Value::Quantity(centered(nominal, dim, percent)?),
        )];
        cells.extend(
            columns[1..]
                .iter()
                .map(|(name, _)| (name.clone(), Value::Open(Sense::Stated))),
        );
        let generic = Generic {
            catalog: catalog.to_string(),
            package: String::new(),
            spec: Spec::Series {
                series,
                column: column.clone(),
                dim,
                nominal,
                tolerance_percent: percent,
                columns: columns.to_vec(),
                needs: Vec::new(),
            },
        };
        out.push(Candidate {
            name: format!("generic {}", generic.describe()),
            label: label(nominal),
            columns: cells,
            generic: Some(generic),
            ..Candidate::default()
        });
    }
    Some(out)
}

fn centered(nominal: f64, dim: Dimension, percent: f64) -> Option<Quantity> {
    let center = Quantity::point(dim, nominal).ok()?;
    let tolerance = Quantity::literal(percent, Some("%")).ok()?.quantity;
    Quantity::tolerance(&center, &tolerance, true).ok()
}

pub(crate) fn resistors(
    standard: &Standard,
    series: Series,
    columns: &[(String, FieldKind)],
) -> Option<Vec<Candidate>> {
    let mut out = Vec::new();
    for (package, power) in PACKAGES {
        for ohms in series.resistances() {
            let generic = Generic {
                catalog: standard.catalog.to_string(),
                package: package.to_string(),
                spec: Spec::Resistor {
                    series,
                    nominal: ohms,
                    tolerance_percent: TOLERANCE_PERCENT,
                    max_power: power,
                },
            };
            let mut values = Vec::new();
            for (name, _) in columns {
                let value = match standard.fill(name)? {
                    Fill::Marked => {
                        Value::Quantity(centered(ohms, Dimension::RESISTANCE, TOLERANCE_PERCENT)?)
                    }
                    Fill::PackagePower => {
                        Value::Quantity(Quantity::point(Dimension::POWER, power).ok()?)
                    }
                    // A generic part has no temperature coefficient: that cell is empty.
                    Fill::Empty => continue,
                    Fill::Package => Value::Text(package.to_string()),
                    Fill::NoCurve | Fill::Open(_) => return None,
                };
                values.push((name.clone(), value));
            }
            out.push(Candidate {
                name: format!("generic {}", generic.describe()),
                label: label(ohms),
                columns: values,
                generic: Some(generic),
                ..Candidate::default()
            });
        }
    }
    Some(out)
}

pub(crate) fn capacitors(
    standard: &Standard,
    series: Series,
    columns: &[(String, FieldKind)],
) -> Option<Vec<Candidate>> {
    // 1 pF up to 100 uF.
    let values: Vec<f64> = series
        .values(-12..=-4)
        .into_iter()
        .filter(|&f| f <= 1e-4 * (1.0 + 1e-9))
        .collect();
    let mut out = Vec::new();
    for package in CAPACITOR_PACKAGES {
        for percent in CAPACITOR_TOLERANCES {
            for &farads in &values {
                let marked = centered(farads, Dimension::CAPACITANCE, percent)?;
                let generic = Generic {
                    catalog: standard.catalog.to_string(),
                    package: package.to_string(),
                    spec: Spec::Capacitor {
                        series,
                        nominal: farads,
                        tolerance_percent: percent,
                        max_voltage: Need::Nothing,
                    },
                };
                let mut cells = Vec::new();
                for (name, _) in columns {
                    let value = match standard.fill(name)? {
                        Fill::Marked => Value::Quantity(marked),
                        // No published curve: the effective capacitance is unknown, as for a
                        // row whose cell is `derate capacitance by bias:` with nothing after.
                        Fill::NoCurve => Value::Derated(Derating {
                            marked,
                            against: "bias".to_string(),
                            curve: Vec::new(),
                        }),
                        Fill::Open(sense) => Value::Open(sense),
                        // A generic part has no dielectric: that cell is empty, so a design
                        // that needs one is given a catalog row.
                        Fill::Empty => continue,
                        Fill::Package => Value::Text(package.to_string()),
                        Fill::PackagePower => return None,
                    };
                    cells.push((name.clone(), value));
                }
                out.push(Candidate {
                    name: format!("generic {}", generic.describe()),
                    label: label(farads),
                    columns: cells,
                    generic: Some(generic),
                    ..Candidate::default()
                });
            }
        }
    }
    Some(out)
}

pub(crate) fn inductors(
    standard: &Standard,
    series: Series,
    columns: &[(String, FieldKind)],
) -> Option<Vec<Candidate>> {
    // 1 nH up to 1 mH.
    let values: Vec<f64> = series
        .values(-9..=-3)
        .into_iter()
        .filter(|&h| h <= 1e-3 * (1.0 + 1e-9))
        .collect();
    let mut out = Vec::new();
    for package in INDUCTOR_PACKAGES {
        for percent in INDUCTOR_TOLERANCES {
            for &henries in &values {
                let marked = centered(henries, Dimension::INDUCTANCE, percent)?;
                let generic = Generic {
                    catalog: standard.catalog.to_string(),
                    package: package.to_string(),
                    spec: Spec::Inductor {
                        series,
                        nominal: henries,
                        tolerance_percent: percent,
                        saturation_current: Need::Nothing,
                        dc_resistance: Need::Nothing,
                    },
                };
                let mut cells = Vec::new();
                for (name, _) in columns {
                    let value = match standard.fill(name)? {
                        Fill::Marked => Value::Quantity(marked),
                        Fill::Open(sense) => Value::Open(sense),
                        // A generic part has no heating current: that cell is empty.
                        Fill::Empty => continue,
                        Fill::Package => Value::Text(package.to_string()),
                        Fill::PackagePower | Fill::NoCurve => return None,
                    };
                    cells.push((name.clone(), value));
                }
                out.push(Candidate {
                    name: format!("generic {}", generic.describe()),
                    label: label(henries),
                    columns: cells,
                    generic: Some(generic),
                    ..Candidate::default()
                });
            }
        }
    }
    Some(out)
}

pub(crate) fn leds(
    standard: &Standard,
    _series: Series,
    columns: &[(String, FieldKind)],
) -> Option<Vec<Candidate>> {
    let mut out = Vec::new();
    for package in LED_PACKAGES {
        let generic = Generic {
            catalog: standard.catalog.to_string(),
            package: package.to_string(),
            spec: Spec::Led {
                color: Need::Nothing,
                forward_voltage: Need::Nothing,
            },
        };
        let cells = columns
            .iter()
            .map(|(name, _)| {
                let value = match standard.fill(name)? {
                    Fill::Open(sense) => Value::Open(sense),
                    Fill::Package => Value::Text(package.to_string()),
                    Fill::Marked | Fill::PackagePower | Fill::NoCurve | Fill::Empty => {
                        return None;
                    }
                };
                Some((name.clone(), value))
            })
            .collect::<Option<Vec<_>>>()?;
        out.push(Candidate {
            name: format!("generic {}", generic.describe()),
            label: "LED".to_string(),
            columns: cells,
            generic: Some(generic),
            ..Candidate::default()
        });
    }
    Some(out)
}

/// The part a generic part resolved to.
#[derive(Debug, Clone, PartialEq)]
pub struct Orderable {
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
    pub library: Option<Library>,
    /// What a message calls the row.
    pub name: String,
    /// The row's `height` in mm, if it has one (ADR 0094).
    pub height: Option<f64>,
    /// The row's `temp_min` and `temp_max` in kelvin, if it has them (ADR 0099).
    pub temp_min: Option<f64>,
    pub temp_max: Option<f64>,
    /// The cited range of the row's dielectric class, if it names one that has a range.
    pub temp_class: Option<&'static netsmith_check::environment::ClassRange>,
    /// The row's `qualification` cell, `Aec` or `None`, if it has one (ADR 0099).
    pub qualification: Option<String>,
    /// Rows that fit but were passed over because a provider said they have no stock.
    pub passed_over: Vec<String>,
}

fn rank(library: Option<Library>) -> u8 {
    match library {
        Some(Library::Basic) => 0,
        Some(Library::Preferred) => 1,
        Some(Library::Extended) => 2,
        None => 3,
    }
}

fn quantity_of(c: &Candidate, key: &str) -> Option<Quantity> {
    match c.column(key) {
        Some(Value::Quantity(q)) => Some(*q),
        _ => None,
    }
}

/// Why a row does not stand for a generic part.
#[derive(Debug, Clone, PartialEq)]
enum Miss {
    Package,
    Value,
    Tolerance,
    /// A rating or a stated value the row does not meet (the column).
    Rating(String),
    /// A capacitor row with no recorded DC-bias curve.
    NoCurve,
}

/// What a row lacks to stand for a generic part: empty if it stands for it.
fn misses(generic: &Generic, row: &Candidate) -> Vec<Miss> {
    let mut out = Vec::new();
    let has_package = !matches!(generic.spec, Spec::Series { .. });
    if has_package
        && !matches!(row.column("package"), Some(Value::Text(p)) if *p == generic.package)
    {
        out.push(Miss::Package);
    }
    if let (Some(nominal), Some(column)) = (generic.nominal(), generic.value_column()) {
        let tolerance = match &generic.spec {
            Spec::Resistor {
                tolerance_percent, ..
            }
            | Spec::Capacitor {
                tolerance_percent, ..
            }
            | Spec::Inductor {
                tolerance_percent, ..
            }
            | Spec::Series {
                tolerance_percent, ..
            } => *tolerance_percent,
            Spec::Led { .. } => 0.0,
        };
        match quantity_of(row, column) {
            Some(q) if (q.mid() - nominal).abs() <= nominal * 1e-6 => {
                let want_lo = nominal * (1.0 - tolerance / 100.0);
                let want_hi = nominal * (1.0 + tolerance / 100.0);
                let slack = nominal * 1e-9;
                if q.lo() < want_lo - slack || q.hi() > want_hi + slack {
                    out.push(Miss::Tolerance);
                }
            }
            _ => out.push(Miss::Value),
        }
    }
    match &generic.spec {
        Spec::Series { needs, .. } => {
            for (name, need) in needs {
                if !need.met_by(row.column(name)) {
                    out.push(Miss::Rating(name.clone()));
                }
            }
        }
        Spec::Resistor { max_power, .. } => {
            if !quantity_of(row, "max_power").is_some_and(|p| p.lo() >= *max_power) {
                out.push(Miss::Rating("max_power".to_string()));
            }
        }
        Spec::Capacitor { max_voltage, .. } => {
            if !max_voltage.met_by(row.column("max_voltage")) {
                out.push(Miss::Rating("max_voltage".to_string()));
            }
            // Owner decision, 2026-11-03: only a row with a recorded DC-bias curve.
            if !matches!(row.column("effective_capacitance"), Some(Value::Derated(d)) if !d.curve.is_empty())
            {
                out.push(Miss::NoCurve);
            }
        }
        Spec::Inductor {
            saturation_current,
            dc_resistance,
            ..
        } => {
            if !saturation_current.met_by(row.column("saturation_current")) {
                out.push(Miss::Rating("saturation_current".to_string()));
            }
            if !dc_resistance.met_by(row.column("dc_resistance")) {
                out.push(Miss::Rating("dc_resistance".to_string()));
            }
        }
        Spec::Led {
            color,
            forward_voltage,
        } => {
            if !color.met_by(row.column("color")) {
                out.push(Miss::Rating("color".to_string()));
            }
            if !forward_voltage.met_by(row.column("forward_voltage")) {
                out.push(Miss::Rating("forward_voltage".to_string()));
            }
        }
    }
    out
}

/// How far a row is from the ratings a generic part asked for, added over the columns:
/// `0` for a row that meets them all. A row near the need ranks first among the rows that
/// fail it, whatever its value (ADR 0047).
fn shortfall(generic: &Generic, row: &Candidate) -> f64 {
    match &generic.spec {
        Spec::Series { needs, .. } => needs
            .iter()
            .map(|(name, need)| need.shortfall(row.column(name)))
            .sum(),
        Spec::Resistor { max_power, .. } => {
            Need::AtLeast(*max_power).shortfall(row.column("max_power"))
        }
        Spec::Capacitor { max_voltage, .. } => {
            let no_curve = !matches!(row.column("effective_capacitance"), Some(Value::Derated(d)) if !d.curve.is_empty());
            max_voltage.shortfall(row.column("max_voltage")) + f64::from(u8::from(no_curve))
        }
        Spec::Inductor {
            saturation_current,
            dc_resistance,
            ..
        } => {
            saturation_current.shortfall(row.column("saturation_current"))
                + dc_resistance.shortfall(row.column("dc_resistance"))
        }
        Spec::Led {
            color,
            forward_voltage,
        } => {
            color.shortfall(row.column("color"))
                + forward_voltage.shortfall(row.column("forward_voltage"))
        }
    }
}

/// Find the row of the catalog that stands for a generic part, or say why there is none.
///
/// A row fits when it has the package, the nominal value (to a millionth), a tolerance no
/// wider, and every rating the part asked for: a resistor's power rating no lower, a
/// capacitor's voltage and (always) a recorded DC-bias curve, an inductor's saturation
/// current and DC resistance, an LED's colour and forward voltage. Among the rows that fit
/// the order is the preferred-parts list: Basic, then Preferred, then Extended, and inside a
/// class the order of the file. `stock` is asked about each fitting row's LCSC number: a row
/// it answers `0` for is passed over, and a row it knows nothing about (`None`) is kept.
///
/// The error is the message for the part: what was asked for, and the nearest rows.
pub fn resolve(
    generic: &Generic,
    catalogs: &Catalogs,
    stock: &dyn Fn(&str) -> Option<u64>,
) -> Result<Orderable, String> {
    let Some(Ok(catalog)) = catalogs.identity(&generic.catalog, &generic.kinds(), &[]) else {
        return Err(format!(
            "unresolved: there is no readable `{}` catalog to find {} in",
            generic.catalog,
            generic.describe()
        ));
    };
    let rows: Vec<&Candidate> = catalog
        .candidates
        .iter()
        .filter(|c| c.lcsc.is_some() || c.mpn.is_some())
        .collect();
    let mut fits: Vec<&Candidate> = rows
        .iter()
        .copied()
        .filter(|c| misses(generic, c).is_empty())
        .collect();
    fits.sort_by_key(|c| rank(c.library));
    let mut passed_over = Vec::new();
    for c in fits.iter().copied() {
        let in_stock = c.lcsc.as_deref().and_then(stock);
        if in_stock == Some(0) {
            passed_over.push(format!("{} has no stock", c.name));
            continue;
        }
        return Ok(Orderable {
            lcsc: c.lcsc.clone(),
            mpn: c.mpn.clone(),
            library: c.library,
            name: c.name.clone(),
            height: c.height,
            temp_min: c.temp_min,
            temp_max: c.temp_max,
            temp_class: c.class_range(),
            qualification: c.qualification.clone(),
            passed_over,
        });
    }

    let mut message = format!(
        "unresolved: no row of the `{}` catalog can be ordered for {}.",
        generic.catalog,
        generic.describe()
    );
    if !passed_over.is_empty() {
        message.push_str(&format!(
            " Rows that fit were passed over: {}.",
            passed_over.join("; ")
        ));
    }
    // A capacitor row that matches in everything but has no recorded DC-bias curve.
    let no_curve: Vec<&str> = rows
        .iter()
        .filter(|c| misses(generic, c) == [Miss::NoCurve])
        .map(|c| c.name.as_str())
        .collect();
    if !no_curve.is_empty() {
        let shown: Vec<&str> = no_curve.iter().copied().take(3).collect();
        message.push_str(&format!(
            " {} row(s) match but have no recorded DC-bias curve, which a generic ceramic capacitor requires: {}{}.",
            no_curve.len(),
            shown.join(", "),
            if no_curve.len() > shown.len() { ", ..." } else { "" }
        ));
    }
    let alternatives = nearest(generic, &rows, &fits);
    if alternatives.is_empty() {
        message.push_str(&format!(
            " The catalog has no {} to offer instead.",
            generic.word()
        ));
    } else {
        message.push_str(&format!(
            " Nearest in the catalog: {}.",
            alternatives
                .iter()
                .map(|a| a.line.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    message.push_str(CHANGE_THE_VALUE);
    Err(message)
}

/// The advice that ends the message of an unresolved part.
pub const CHANGE_THE_VALUE: &str = " Change the value in the design and build again: the solver then checks every relation again with it.";

/// The rows nearest a generic part that do not stand for it, up to three, or none if the
/// catalog cannot be read. These are the alternatives the message offers and that a build
/// can check the design against.
pub fn alternatives(generic: &Generic, catalogs: &Catalogs) -> Vec<Alternative> {
    let Some(Ok(catalog)) = catalogs.identity(&generic.catalog, &generic.kinds(), &[]) else {
        return Vec::new();
    };
    let rows: Vec<&Candidate> = catalog
        .candidates
        .iter()
        .filter(|c| c.lcsc.is_some() || c.mpn.is_some())
        .collect();
    let fits: Vec<&Candidate> = rows
        .iter()
        .copied()
        .filter(|c| misses(generic, c).is_empty())
        .collect();
    nearest(generic, &rows, &fits)
}

/// A row near a generic part, for a message and for the re-check of an alternative.
#[derive(Debug, Clone, PartialEq)]
pub struct Alternative {
    /// The row's name.
    pub name: String,
    pub lcsc: Option<String>,
    pub mpn: Option<String>,
    /// The row's value, in the base unit, if the kind has one.
    pub value: Option<f64>,
    /// `330Ω Basic (-7.7%, Basic)`.
    pub line: String,
}

/// The three rows nearest a generic part that do not stand for it: an LED's colour first,
/// then the rows nearest the ratings that the design needs (a row that falls a little short
/// of the saturation current ahead of one that falls far short, whatever its value), then
/// the same package, then nearest in value, then Basic first.
pub fn nearest(generic: &Generic, rows: &[&Candidate], fits: &[&Candidate]) -> Vec<Alternative> {
    let column = generic.value_column();
    let nominal = generic.nominal();
    let same_package = |c: &Candidate| matches!(c.column("package"), Some(Value::Text(p)) if *p == generic.package);
    let same_color = |c: &Candidate| match &generic.spec {
        Spec::Led {
            color: Need::Text(t),
            ..
        } => matches!(c.column("color"), Some(Value::Text(v)) if v == t),
        _ => true,
    };
    let mut near: Vec<(&Candidate, f64, f64)> = rows
        .iter()
        .copied()
        .filter(|c| !fits.iter().any(|f| std::ptr::eq(*f, *c)))
        .filter_map(|c| match (column, nominal) {
            (Some(col), Some(n)) => {
                let v = quantity_of(c, col)?.mid();
                (v > 0.0).then(|| (c, shortfall(generic, c), (v / n).ln().abs()))
            }
            _ => Some((c, shortfall(generic, c), 0.0)),
        })
        .collect();
    near.sort_by(|a, b| {
        (!same_color(a.0))
            .cmp(&!same_color(b.0))
            .then(a.1.total_cmp(&b.1))
            .then((!same_package(a.0)).cmp(&!same_package(b.0)))
            .then(a.2.total_cmp(&b.2))
            .then(rank(a.0.library).cmp(&rank(b.0.library)))
    });
    near.iter()
        .take(3)
        .map(|(c, _, _)| {
            let value = column.and_then(|col| quantity_of(c, col)).map(|q| q.mid());
            let library = c.library.map(|l| format!(", {l}")).unwrap_or_default();
            let line = match (value, nominal) {
                (Some(v), Some(n)) => {
                    format!("{} ({:+.1}%{library})", c.name, (v - n) / n * 100.0)
                }
                _ => format!(
                    "{}{}",
                    c.name,
                    c.library.map(|l| format!(" ({l})")).unwrap_or_default()
                ),
            };
            Alternative {
                name: c.name.clone(),
                lcsc: c.lcsc.clone(),
                mpn: c.mpn.clone(),
                value,
                line,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_series_have_their_standard_number_of_values_in_each_decade() {
        assert_eq!(Series::E12.decade().len(), 12);
        assert_eq!(Series::E24.decade().len(), 24);
        assert_eq!(Series::E96.decade().len(), 96);
        let v = Series::E24.resistances();
        assert_eq!(v.len(), 24 * 7);
        assert!(v.windows(2).all(|w| w[0] < w[1]), "rising");
        assert!(v.contains(&330.0) && v.contains(&2200.0) && v.contains(&9.1e6));
    }

    #[test]
    fn a_value_is_written_the_way_the_catalog_rows_write_it() {
        assert_eq!(label(330.0), "330");
        assert_eq!(label(2200.0), "2.2k");
        assert_eq!(label(1e6), "1M");
        assert_eq!(label(4.7), "4.7");
    }

    #[test]
    fn series_are_read_in_any_case() {
        assert_eq!(Series::parse("e96"), Some(Series::E96));
        assert_eq!(Series::parse("E48"), None);
    }
}
