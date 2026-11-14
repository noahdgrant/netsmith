//! What the solver knows about the standard catalogs that have synthesized parts (ADR 0068
//! stage 3, ADR 0125).
//!
//! The catalogs `resistor`, `capacitor`, `inductor` and `led` have, besides their rows, a
//! provider with no supplier, the synthesized one of ADR 0047. How it makes its parts (the
//! packages, the tolerance kinds, the package's power rating, a capacitance with no bias
//! curve) is a property of the catalog, not of a definition, and the language cannot say it,
//! so it is Rust, and this is the one place that names the four catalogs. Any part that lists
//! one of them in its `catalog` statement gets these parts, a project's own part too.
//!
//! Which E-series the values come from is the definition's, not Rust's: its
//! `#[series(E12, E24, E96)]` (ADR 0125), with the project's `series` key choosing among them
//! ([`Series::within`]). A part whose definition offers no series is given no synthesized
//! parts of a catalog that is made of a series; an LED has no value, and its catalog's parts
//! need no series. A project's own catalog with a `#[series(...)]` gets the plain values of
//! the series (`generic::Spec::Series`).
//!
//! Everything else that used to be written four times is a field of an entry here:
//!
//! - the columns the synthesized provider can fill, and how it fills each of them, which is
//!   also the check that a part's `catalog` statement lists only columns it can fill;
//! - the kind of each column when a row is read again for the bill of materials;
//! - the column that holds the value a design is expected to choose or bound (ADR 0065);
//! - the columns a file of rows may leave out (ADR 0055, ADR 0058): they are the ones the
//!   synthesized provider leaves empty. A synthesized part has no `temperature_coefficient`,
//!   no `dielectric` and no `rms_current`, so a row of the catalog that is empty there must be
//!   offered to a design that does not ask for the column, or the synthesized part, which is
//!   empty there too, would fit no design at all.
//!
//! A catalog is a name that every part listing it shares (ADR 0022: `resistor.csv` is read by
//! every part with `catalog resistor`), so the entries are found by the catalog's name. A test
//! elaborates the standard generic that owns each catalog and compares the columns, so an
//! entry cannot drift from the definition it belongs to.

use netsmith_check::FieldKind;
use netsmith_quantity::Dimension;

use crate::catalog::Candidate;
use crate::generic::{self, Sense, Series, Spec};

/// How a synthesized part fills one column of its catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fill {
    /// The marked value of the series, with the tolerance of the part: the number the part is
    /// chosen by.
    Marked,
    /// The power the part's package is usually rated for (a resistor's `max_power`).
    PackagePower,
    /// The marked value as a capacitance that derates with the bias, with no curve: a generic
    /// capacitor has none, and resolves only to a row that has one.
    NoCurve,
    /// A rating or a stated value the part cannot know, left open for the design to supply
    /// when the part is picked.
    Open(Sense),
    /// The name of the package.
    Package,
    /// Nothing: the part has no such value, and the cell is empty.
    Empty,
}

/// One column of a standard catalog.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Column {
    pub name: &'static str,
    /// What a cell is read as when a row is read again to resolve a generic part.
    pub kind: FieldKind,
    pub fill: Fill,
    /// Whether the column holds the part's value, the number a design is expected to choose or
    /// bound (ADR 0065): a pick of such a part that nothing limits is a guideline finding. A
    /// rating is not a value.
    pub value: bool,
}

/// Makes the synthesized parts of a series for the columns a part's catalog lists.
type Make = fn(&Standard, Series, &[(String, FieldKind)]) -> Option<Vec<Candidate>>;

/// A standard generic's catalog and its synthesized provider.
pub(crate) struct Standard {
    /// The catalog it reads (`resistor`).
    pub catalog: &'static str,
    /// What a message calls one of its parts.
    pub word: &'static str,
    /// The columns of the generic's `catalog` statement, in its order.
    pub columns: &'static [Column],
    /// The synthesized parts of a series, in the order they are tried.
    make: Make,
}

const fn quantity(name: &'static str, dim: Dimension, fill: Fill, value: bool) -> Column {
    Column {
        name,
        kind: FieldKind::Quantity(dim),
        fill,
        value,
    }
}

const fn text(name: &'static str, fill: Fill) -> Column {
    Column {
        name,
        kind: FieldKind::Text,
        fill,
        value: false,
    }
}

/// `catalog resistor { .resistance, .max_power, .package, .temperature_coefficient }`.
static RESISTOR: Standard = Standard {
    catalog: "resistor",
    word: "resistor",
    columns: &[
        quantity("resistance", Dimension::RESISTANCE, Fill::Marked, true),
        quantity("max_power", Dimension::POWER, Fill::PackagePower, false),
        text("package", Fill::Package),
        // Read with the others; a generic part has none and no row is matched on it.
        quantity(
            "temperature_coefficient",
            Dimension::NONE,
            Fill::Empty,
            false,
        ),
    ],
    make: generic::resistors,
};

/// `catalog capacitor { .capacitance, .effective_capacitance, .max_voltage, .package,
/// .dielectric }`.
static CAPACITOR: Standard = Standard {
    catalog: "capacitor",
    word: "capacitor",
    columns: &[
        quantity("capacitance", Dimension::CAPACITANCE, Fill::Marked, true),
        quantity(
            "effective_capacitance",
            Dimension::CAPACITANCE,
            Fill::NoCurve,
            true,
        ),
        quantity(
            "max_voltage",
            Dimension::VOLTAGE,
            Fill::Open(Sense::AtLeast),
            false,
        ),
        text("package", Fill::Package),
        // Read with the others; a generic part has none and no row is matched on it. It is an
        // enum in the part and text here.
        text("dielectric", Fill::Empty),
    ],
    make: generic::capacitors,
};

/// `catalog inductor { .inductance, .saturation_current, .dc_resistance, .package,
/// .rms_current }`.
static INDUCTOR: Standard = Standard {
    catalog: "inductor",
    word: "inductor",
    columns: &[
        quantity("inductance", Dimension::INDUCTANCE, Fill::Marked, true),
        quantity(
            "saturation_current",
            Dimension::CURRENT,
            Fill::Open(Sense::AtLeast),
            false,
        ),
        quantity(
            "dc_resistance",
            Dimension::RESISTANCE,
            Fill::Open(Sense::AtMost),
            false,
        ),
        text("package", Fill::Package),
        quantity("rms_current", Dimension::CURRENT, Fill::Empty, false),
    ],
    make: generic::inductors,
};

/// `catalog led { .color, .forward_voltage, .package }`.
static LED: Standard = Standard {
    catalog: "led",
    word: "LED",
    columns: &[
        text("color", Fill::Open(Sense::Stated)),
        quantity(
            "forward_voltage",
            Dimension::VOLTAGE,
            Fill::Open(Sense::Stated),
            false,
        ),
        text("package", Fill::Package),
    ],
    make: generic::leds,
};

/// The four, in the order of the standard library.
static STANDARD: [&Standard; 4] = [&RESISTOR, &CAPACITOR, &LED, &INDUCTOR];

impl Standard {
    /// The standard catalog of this name, if it has synthesized parts.
    pub(crate) fn of_catalog(name: &str) -> Option<&'static Standard> {
        STANDARD.iter().copied().find(|s| s.catalog == name)
    }

    /// The standard catalog a synthesized part is of; none for a part of a project's own
    /// catalog that declares `#[series(...)]`.
    pub(crate) fn of_spec(spec: &Spec) -> Option<&'static Standard> {
        match spec {
            Spec::Resistor { .. } => Some(&RESISTOR),
            Spec::Capacitor { .. } => Some(&CAPACITOR),
            Spec::Inductor { .. } => Some(&INDUCTOR),
            Spec::Led { .. } => Some(&LED),
            Spec::Series { .. } => None,
        }
    }

    pub(crate) fn column(&self, name: &str) -> Option<&'static Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// How a synthesized part fills the column, if it can.
    pub(crate) fn fill(&self, name: &str) -> Option<Fill> {
        self.column(name).map(|c| c.fill)
    }

    /// The column that holds the marked value the series is made of, for a kind that has one:
    /// an LED has none.
    pub(crate) fn marked(&self) -> Option<&'static str> {
        self.columns
            .iter()
            .find(|c| c.fill == Fill::Marked)
            .map(|c| c.name)
    }

    /// The columns that hold the part's value (ADR 0065).
    pub(crate) fn value_columns(&self) -> Vec<&'static str> {
        self.columns
            .iter()
            .filter(|c| c.value)
            .map(|c| c.name)
            .collect()
    }

    /// Whether a file of rows may leave the column out, and a row may be empty in it, without
    /// the row leaving every design that does not mention it (ADR 0055).
    pub(crate) fn is_optional(&self, column: &str) -> bool {
        self.fill(column) == Some(Fill::Empty)
    }

    /// The columns a row is read with when it is resolved for a generic part.
    pub(crate) fn kinds(&self) -> Vec<(String, FieldKind)> {
        self.columns
            .iter()
            .map(|c| (c.name.to_string(), c.kind))
            .collect()
    }

    /// The synthesized parts of `series` for a part whose catalog statement lists `columns`, in
    /// the order they are tried, if it lists only columns they can fill and the one the series
    /// is made of. `None` otherwise.
    pub(crate) fn candidates(
        &'static self,
        series: Series,
        columns: &[(String, FieldKind)],
    ) -> Option<Vec<Candidate>> {
        if !columns.iter().all(|(name, _)| self.column(name).is_some()) {
            return None;
        }
        if let Some(marked) = self.marked()
            && !columns.iter().any(|(name, _)| name == marked)
        {
            return None;
        }
        (self.make)(self, series, columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netsmith_check::{Loader, check};

    /// The standard generic whose `catalog` statement an entry describes.
    fn generic_of(standard: &Standard) -> &'static str {
        match standard.catalog {
            "resistor" => "Resistor",
            "capacitor" => "Capacitor",
            "inductor" => "Inductor",
            "led" => "Led",
            other => panic!("no standard generic for the catalog `{other}`"),
        }
    }

    /// The columns of the `catalog` statement of each standard generic, in the order it lists
    /// them, as the checker reads them from the standard library.
    fn declared(generic: &str, module: &str) -> (String, Vec<String>) {
        let (name, columns, _) = declared_with_optional(generic, module);
        (name, columns)
    }

    /// [`declared`], and the columns that the statement marks `optional` (ADR 0078).
    fn declared_with_optional(generic: &str, module: &str) -> (String, Vec<String>, Vec<String>) {
        let source =
            format!("import std::parts::{module}::{generic}\nboard B {{\n    .x: {generic}\n}}\n");
        let checked = check("main.nsm", &source, &Loader::std());
        assert!(checked.is_ok(), "{}", checked.render());
        let model = &checked.boards[0].model;
        let part = model.inst_at("x").expect("the part is placed");
        let catalog = model
            .instance(part)
            .catalog
            .as_ref()
            .expect("a standard generic has a catalog");
        let prefix = format!("{}.", model.path_of(part));
        let name_of = |&f: &_| {
            let path = model.path_of_field(f);
            path.strip_prefix(&prefix).unwrap_or(&path).to_string()
        };
        let columns = catalog.fields.iter().map(name_of).collect();
        let optional = catalog.optional.iter().map(name_of).collect();
        (catalog.name.clone(), columns, optional)
    }

    #[test]
    fn the_standard_catalog_statements_mark_optional_the_columns_a_synthesized_part_leaves_empty() {
        // ADR 0078: `optional` in the `catalog` statement and `Fill::Empty` here say the same
        // thing for the four standard kinds, so the marker in the library is not a second
        // truth that could drift from the rule the solver also keeps under the catalog's name.
        for standard in STANDARD {
            let (_, _, marked) = declared_with_optional(generic_of(standard), "passive");
            let ours: Vec<&str> = standard
                .columns
                .iter()
                .filter(|c| standard.is_optional(c.name))
                .map(|c| c.name)
                .collect();
            assert_eq!(marked, ours, "{}", generic_of(standard));
        }
    }

    #[test]
    fn each_entry_describes_the_catalog_statement_of_its_standard_generic() {
        for standard in STANDARD {
            let (catalog, columns) = declared(generic_of(standard), "passive");
            assert_eq!(catalog, standard.catalog, "{}", generic_of(standard));
            let ours: Vec<&str> = standard.columns.iter().map(|c| c.name).collect();
            assert_eq!(columns, ours, "{}", generic_of(standard));
        }
    }

    #[test]
    fn a_capacitor_bank_reads_the_catalog_of_a_capacitor_column_for_column() {
        // ADR 0123: the members of a bank are `Capacitor`s of one row, so the bank's `catalog`
        // statement and `Capacitor`'s are one, and the `CAPACITOR` entry serves both.
        assert_eq!(
            declared_with_optional("CapacitorBank", "passive"),
            declared_with_optional("Capacitor", "passive")
        );
    }

    #[test]
    fn each_entry_makes_parts_for_the_columns_of_its_generic() {
        for standard in STANDARD {
            let (_, columns) = declared(generic_of(standard), "passive");
            let kinds: Vec<(String, FieldKind)> = columns
                .iter()
                .map(|n| (n.clone(), standard.column(n).unwrap().kind))
                .collect();
            let made = standard
                .candidates(Series::E24, &kinds)
                .unwrap_or_else(|| panic!("{} makes no part", generic_of(standard)));
            assert!(!made.is_empty(), "{}", generic_of(standard));
            // A synthesized part is empty exactly where a row may be.
            for part in &made {
                for column in standard.columns {
                    assert_eq!(
                        part.column(column.name).is_none(),
                        standard.is_optional(column.name),
                        "{} {}",
                        generic_of(standard),
                        column.name
                    );
                }
            }
        }
    }

    #[test]
    fn a_catalog_that_lists_a_column_the_part_cannot_fill_has_no_synthesized_parts() {
        let mut kinds = RESISTOR.kinds();
        kinds.push(("tolerance".into(), FieldKind::Text));
        assert!(RESISTOR.candidates(Series::E24, &kinds).is_none());
        kinds.truncate(RESISTOR.columns.len());
        kinds.remove(0);
        assert!(
            RESISTOR.candidates(Series::E24, &kinds).is_none(),
            "no value to build the series from"
        );
    }

    #[test]
    fn the_columns_a_file_may_leave_out_and_the_value_columns_are_the_ones_the_adrs_name() {
        // ADR 0055 and ADR 0058: the columns added after catalog files were written.
        let optional: Vec<(&str, &str)> = STANDARD
            .iter()
            .flat_map(|s| {
                s.columns
                    .iter()
                    .filter(|c| s.is_optional(c.name))
                    .map(|c| (s.catalog, c.name))
            })
            .collect();
        assert_eq!(
            optional,
            [
                ("resistor", "temperature_coefficient"),
                ("capacitor", "dielectric"),
                ("inductor", "rms_current"),
            ]
        );
        // ADR 0065: the number a design is expected to choose or bound.
        assert_eq!(RESISTOR.value_columns(), ["resistance"]);
        assert_eq!(
            CAPACITOR.value_columns(),
            ["capacitance", "effective_capacitance"]
        );
        assert_eq!(INDUCTOR.value_columns(), ["inductance"]);
        assert!(LED.value_columns().is_empty());
        assert_eq!(
            [
                RESISTOR.marked(),
                CAPACITOR.marked(),
                INDUCTOR.marked(),
                LED.marked()
            ],
            [
                Some("resistance"),
                Some("capacitance"),
                Some("inductance"),
                None
            ]
        );
    }

    #[test]
    fn only_the_four_have_an_entry_and_the_names_are_found_both_ways() {
        for standard in STANDARD {
            assert!(std::ptr::eq(
                Standard::of_catalog(standard.catalog).unwrap(),
                standard
            ));
        }
        assert!(Standard::of_catalog("ldo").is_none());
        assert!(Standard::of_catalog("Resistor").is_none());
    }
}
