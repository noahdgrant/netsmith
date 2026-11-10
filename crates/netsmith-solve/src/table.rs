//! Catalog files: one CSV per catalog, one row per real part (ADR 0022).
//!
//! The first line names the columns. Any of the fixed columns below may be there; every
//! other column is a field of the part, named by its path without the leading dot
//! (`resistance`, `vout.voltage`), and must be one of the columns of the part's
//! `catalog X { ... }` declaration. A file is read in two steps because only the part says
//! what a cell means:
//!
//! 1. [`Table::parse`] reads the file alone and checks what needs no part: the fixed
//!    columns, a source for every row that names a real part, and that a part is listed once.
//! 2. [`Table::candidates`] reads the cells against the part's declared columns: a cell of a
//!    quantity column is a value in the language's syntax (`10kohm +/- 1%`, `1.8V..5.5V`)
//!    of the right dimension, and a cell of a text column is its text, as written.
//!
//! **The order of the rows is the selection policy.** The solver takes the first candidate
//! that works, and nothing here sorts. Write the rows in the order you want them tried:
//! Basic parts before Extended, then cheapest first, and so on.
//!
//! | fixed column | meaning |
//! |---|---|
//! | `lcsc` | the supplier's part number (`C25104`) |
//! | `mpn` | the manufacturer's part number; also a field of the part if its catalog lists `.mpn` |
//! | `library` | JLCPCB's `Basic`, `Extended` or `Preferred` |
//! | `assembly` | who assembles the part when the design does not say: `fab`, `hand`, `none` or `consigned` (ADR 0024, ADR 0084) |
//! | `height` | how tall the part's body stands above the face of the board it is on, a length of one value (`2.4mm`), read from the datasheet or the manufacturer's drawing, with the document, its revision and the page or table in the `source` cell (ADR 0094). The part's `.height` is this when nothing in the design states one; empty means the height is unknown. A part whose `catalog` lists `.height` as a column of its own reads it as that column instead |
//! | `price`, `stock` | the board house's (JLCPCB's) snapshot, USD and pieces, entered by hand; `netsmith catalog refresh` never writes them (ADR 0039). The solver does not read them: a row is offered whatever its stock (ADR 0047), and only the bill of materials shows them, after a supply provider's answer |
//! | `price_breaks` | the board house's price at each quantity, `1:0.0019 1000:0.0015` (pieces:USD), entered by hand; nothing reads it |
//! | `source` | where the row was read. Required when the row has an `lcsc` or `mpn` |
//! | `date` | the day the `price` and `stock` were read, `2026-10-31`; the bill of materials and `netsmith catalog age` take the age of the numbers from it |
//! | `checked` | the day `netsmith catalog refresh` last found the row's manufacturer's number and package equal to LCSC's record, `2026-11-02`; empty if never; nothing reads it |
//! | `name` | what a message calls the row (default: the `mpn`, then the `lcsc`) |
//! | `label` | the schematic's value field (default: the `mpn`, then the `lcsc`, then the `name`) |
//! | `notes` | free text |

use std::collections::BTreeMap;
use std::rc::Rc;

use netsmith_check::FieldKind;
use netsmith_lang::ast::{BinOp, Expr};
use netsmith_lang::parse_expr;
use netsmith_quantity::{Dimension, Quantity, parse_unit};

use crate::catalog::{Candidate, Derating, Library, Value};
use crate::standard::Standard;

/// The columns that are about the row, not about the part.
const FIXED: [&str; 14] = [
    "lcsc",
    "mpn",
    "library",
    "assembly",
    "height",
    "price",
    "price_breaks",
    "stock",
    "source",
    "date",
    "checked",
    "name",
    "label",
    "notes",
];

/// The value columns of a standard kind's catalog, if it has any: the columns that carry the
/// part's value, the number a design is expected to choose or bound (ADR 0065), a resistor's
/// resistance, a capacitor's marked and effective capacitance, an inductor's inductance. A pick
/// of such a part that nothing limits is a guideline finding. A rating (`max_power`,
/// `max_voltage`) is not a value, and the catalogs of other kinds (a Zener's voltage, a
/// crystal's frequency) are rows that a design names or states; they get an entry in
/// [`Standard`] when their use shows the same failure.
pub(crate) fn value_columns(catalog: &str) -> Vec<&'static str> {
    Standard::of_catalog(catalog).map_or_else(Vec::new, Standard::value_columns)
}

/// Whether a column of a catalog may be empty in a row without taking the row out of every
/// design that does not mention it, and may be missing from a file: the columns added after
/// files were written (ADR 0055, ADR 0058), so that a project's catalog that predates one
/// stays valid. `optional` lists the columns the part's `catalog` statement marks `optional`
/// (ADR 0078), the way a part, standard or a project's, declares it. The columns of the
/// standard kinds that have synthesized parts (`Standard`) are also optional under the
/// standard catalog's name, as they were before the marker, so that a project's part that
/// lists one without it is read as before. Any other empty cell is an unknown value of
/// something every design needs (a package, a resistance, a rating), so the row is never
/// offered, and any other column the part lists must be in the file.
pub(crate) fn is_optional(catalog: &str, column: &str, optional: &[String]) -> bool {
    optional.iter().any(|o| o == column)
        || Standard::of_catalog(catalog).is_some_and(|s| s.is_optional(column))
}

/// The fields of one CSV record and the 1-based line it starts on.
pub type Record = (usize, Vec<String>);

/// Split CSV text into records of fields, each with the 1-based line it starts on. A field
/// may be quoted, and a quoted field may hold commas, newlines and doubled quotes (`""` is
/// one `"`).
pub fn parse_csv(text: &str) -> Result<Vec<Record>, String> {
    records(text).map_err(|(line, why)| format!("line {line}: {why}"))
}

/// [`parse_csv`], with the line of a problem apart from what it is.
fn records(text: &str) -> Result<Vec<Record>, (usize, String)> {
    let mut records: Vec<Record> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    // Whether the current record has any content, so a blank line is not a record.
    let mut started = false;
    let (mut line, mut record_line, mut quote_line) = (1, 1, 1);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            line += 1;
        }
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        if !started && !matches!(c, '\n' | '\r') {
            record_line = line;
        }
        match c {
            '"' if field.is_empty() => {
                quoted = true;
                quote_line = line;
                started = true;
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                started = true;
            }
            '\r' => {}
            '\n' => {
                if started || !field.is_empty() {
                    record.push(std::mem::take(&mut field));
                    records.push((record_line, std::mem::take(&mut record)));
                }
                started = false;
            }
            _ => {
                field.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err((quote_line, "a quoted field is never closed".to_string()));
    }
    if started || !field.is_empty() {
        record.push(field);
        records.push((record_line, record));
    }
    Ok(records)
}

/// The names in the first line of a file.
#[derive(Debug)]
struct Header {
    line: usize,
    names: Vec<String>,
}

/// What a catalog file records about one LCSC number, as it was last checked: stock and price
/// move, so these are for a person to read and not for the solver.
#[derive(Debug, Clone, PartialEq)]
pub struct Supplier {
    pub lcsc: String,
    pub mpn: String,
    pub library: Option<Library>,
    /// Pieces in stock when the row was checked, if the row says.
    pub stock: Option<u64>,
    /// US dollars for one piece, if the row says.
    pub unit_price_usd: Option<f64>,
    /// The day the `price` and `stock` were read.
    pub date: Option<String>,
    /// The file and line it is from, as a message names it.
    pub at: String,
}

/// One row of a catalog file: where it came from, its fixed columns, and its cells.
#[derive(Debug, Clone)]
struct Row {
    file: Rc<str>,
    line: usize,
    header: Rc<Header>,
    cells: Vec<String>,
    lcsc: String,
    mpn: String,
    library: Option<Library>,
    name: String,
    label: String,
}

impl Row {
    fn at(&self) -> String {
        format!("{}:{}", self.file, self.line)
    }

    fn cell(&self, column: &str) -> Option<&str> {
        let i = self.header.names.iter().position(|n| n == column)?;
        let cell = self.cells[i].trim();
        (!cell.is_empty()).then_some(cell)
    }
}

/// The rows of one catalog, from one file or several (a project's file adds its rows after
/// the standard ones).
#[derive(Debug, Clone, Default)]
pub struct Table {
    rows: Vec<Row>,
}

impl Table {
    /// What the rows say about one LCSC number: the first row that has it.
    pub fn supplier(&self, lcsc: &str) -> Option<Supplier> {
        let row = self.rows.iter().find(|r| r.lcsc == lcsc)?;
        Some(Supplier {
            lcsc: row.lcsc.clone(),
            mpn: row.mpn.clone(),
            library: row.library,
            stock: row.cell("stock").and_then(|s| s.parse().ok()),
            unit_price_usd: row.cell("price").and_then(|s| s.parse().ok()),
            date: row.cell("date").map(str::to_string),
            at: row.at(),
        })
    }

    /// Read one file. `file` is what messages call it (`catalogs/acme.csv`).
    pub fn parse(file: &str, text: &str) -> Result<Table, String> {
        let records = records(text).map_err(|(line, why)| format!("{file}:{line}: {why}"))?;
        let Some(((line, names), body)) = records.split_first() else {
            return Err(format!(
                "{file}: the file is empty; the first line names the columns"
            ));
        };
        let names: Vec<String> = names.iter().map(|n| n.trim().to_string()).collect();
        for (i, n) in names.iter().enumerate() {
            if n.is_empty() {
                return Err(format!("{file}:{line}: column {} has no name", i + 1));
            }
            if names[..i].contains(n) {
                return Err(format!("{file}:{line}: the column `{n}` is there twice"));
            }
        }
        let header = Rc::new(Header { line: *line, names });
        let file: Rc<str> = file.into();
        let mut rows: Vec<Row> = Vec::new();
        for (line, fields) in body {
            let at = format!("{file}:{line}");
            if fields.len() != header.names.len() {
                return Err(format!(
                    "{at}: the row has {} fields but the header (line {}) has {}",
                    fields.len(),
                    header.line,
                    header.names.len()
                ));
            }
            let mut row = Row {
                file: file.clone(),
                line: *line,
                header: header.clone(),
                cells: fields.clone(),
                lcsc: String::new(),
                mpn: String::new(),
                library: None,
                name: String::new(),
                label: String::new(),
            };
            row.read_fixed_columns()?;
            rows.push(row);
        }
        let mut table = Table::default();
        table.append(Table { rows })?;
        Ok(table)
    }

    /// Add the rows of another file after these. A part may be in a catalog once, whichever
    /// file it is in: the same `lcsc` or the same `mpn` twice is an error naming both lines.
    pub fn append(&mut self, other: Table) -> Result<(), String> {
        let mut by_lcsc: BTreeMap<&str, &Row> = BTreeMap::new();
        let mut by_mpn: BTreeMap<&str, &Row> = BTreeMap::new();
        for row in &self.rows {
            if !row.lcsc.is_empty() {
                by_lcsc.entry(&row.lcsc).or_insert(row);
            }
            if !row.mpn.is_empty() {
                by_mpn.entry(&row.mpn).or_insert(row);
            }
        }
        let mut added: Vec<&Row> = Vec::new();
        for row in &other.rows {
            let twin = |key: &str, what: &str, by: &BTreeMap<&str, &Row>| {
                if key.is_empty() {
                    return Ok(());
                }
                let earlier = by
                    .get(key)
                    .copied()
                    .or_else(|| added.iter().copied().find(|r| field_of(r, what) == key));
                match earlier {
                    Some(first) => Err(format!(
                        "{}: the {what} `{key}` is already a row at {}; a part is in a catalog once",
                        row.at(),
                        first.at()
                    )),
                    None => Ok(()),
                }
            };
            twin(&row.lcsc, "lcsc", &by_lcsc)?;
            twin(&row.mpn, "mpn", &by_mpn)?;
            added.push(row);
        }
        self.rows.extend(other.rows);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The candidates for a part whose `catalog name { ... }` lists `columns`, in the order
    /// of the rows. Every cell is read as its column's kind, and a problem names the file,
    /// the line and the cell. `optional` names the columns the statement marks `optional`
    /// (ADR 0078): a file may leave one out, and a row may leave its cell empty.
    pub fn candidates(
        &self,
        name: &str,
        columns: &[(String, FieldKind)],
        optional: &[String],
    ) -> Result<Vec<Candidate>, String> {
        let listed = || {
            columns
                .iter()
                .map(|(c, _)| format!(".{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut out = Vec::new();
        for row in &self.rows {
            for column in &row.header.names {
                let is_fixed = FIXED.contains(&column.as_str());
                if !is_fixed && !columns.iter().any(|(c, _)| c == column) {
                    return Err(format!(
                        "{}:{}: `{column}` is not a column of `catalog {name}`, which lists {}",
                        row.file,
                        row.header.line,
                        listed()
                    ));
                }
            }
            let mut values = Vec::new();
            for (column, kind) in columns {
                if !row.header.names.contains(column) && is_optional(name, column, optional) {
                    continue;
                }
                if !row.header.names.contains(column) {
                    return Err(format!(
                        "{}:{}: there is no `{column}` column, which `catalog {name}` lists ({})",
                        row.file,
                        row.header.line,
                        listed()
                    ));
                }
                if let Some(value) = row.value(name, column, *kind, columns)? {
                    values.push((column.clone(), value));
                }
            }
            out.push(Candidate {
                name: row.name.clone(),
                label: row.label.clone(),
                columns: values,
                lcsc: (!row.lcsc.is_empty()).then(|| row.lcsc.clone()),
                mpn: (!row.mpn.is_empty()).then(|| row.mpn.clone()),
                library: row.library,
                assembly: row.cell("assembly").map(str::to_string),
                height: if columns.iter().any(|(c, _)| c == "height") {
                    None
                } else {
                    row.height()?
                },
                generic: None,
            });
        }
        Ok(out)
    }
}

fn field_of<'a>(row: &'a Row, what: &str) -> &'a str {
    if what == "lcsc" { &row.lcsc } else { &row.mpn }
}

impl Row {
    /// Read the fixed columns and check what a row must say about itself.
    fn read_fixed_columns(&mut self) -> Result<(), String> {
        let at = self.at();
        self.lcsc = self.cell("lcsc").unwrap_or_default().to_string();
        self.mpn = self.cell("mpn").unwrap_or_default().to_string();
        self.library = match self.cell("library") {
            None => None,
            Some("Basic") => Some(Library::Basic),
            Some("Extended") => Some(Library::Extended),
            Some("Preferred") => Some(Library::Preferred),
            Some(other) => {
                return Err(format!(
                    "{at}: library `{other}` is not Basic, Extended or Preferred"
                ));
            }
        };
        if let Some(assembly) = self.cell("assembly")
            && !["fab", "hand", "none", "consigned"].contains(&assembly)
        {
            return Err(format!(
                "{at}: assembly `{assembly}` is not fab, hand, none or consigned"
            ));
        }
        if let Some(price) = self.cell("price")
            && !price
                .parse::<f64>()
                .is_ok_and(|p| p.is_finite() && p >= 0.0)
        {
            return Err(format!("{at}: price `{price}` is not a number of dollars"));
        }
        if let Some(stock) = self.cell("stock")
            && stock.parse::<u64>().is_err()
        {
            return Err(format!("{at}: stock `{stock}` is not a whole number"));
        }
        for column in ["date", "checked"] {
            if let Some(date) = self.cell(column)
                && !is_date(date)
            {
                return Err(format!("{at}: {column} `{date}` is not like 2026-10-31"));
            }
        }
        let real = !self.lcsc.is_empty() || !self.mpn.is_empty();
        if real && self.cell("source").is_none() {
            let which = if self.lcsc.is_empty() {
                format!("mpn {}", self.mpn)
            } else {
                format!("lcsc {}", self.lcsc)
            };
            return Err(format!(
                "{at}: the row for {which} has no source; every part in a catalog needs the URL or text it was read from in its `source` column"
            ));
        }
        let first =
            |names: [Option<&str>; 3]| names.into_iter().flatten().next().map(str::to_string);
        let (lcsc, mpn) = (self.cell("lcsc"), self.cell("mpn"));
        let name = first([self.cell("name"), mpn, lcsc]);
        let Some(name) = name else {
            return Err(format!(
                "{at}: the row has no lcsc, mpn or name, so a message could not say which part it is"
            ));
        };
        self.label = first([self.cell("label"), mpn, lcsc]).unwrap_or_else(|| name.clone());
        self.name = name;
        Ok(())
    }

    /// The `height` cell: how tall the part stands, in mm, or `None` when the cell is empty. It
    /// is one length that is not negative (ADR 0094): a range or a tolerance would make a
    /// limit's check depend on which end was meant.
    fn height(&self) -> Result<Option<f64>, String> {
        let Some(cell) = self.cell("height") else {
            return Ok(None);
        };
        let bad = |why: String| format!("{}: `height`: {why}", self.at());
        let q = quantity(cell).map_err(bad)?;
        if q.dim() != Dimension::LENGTH {
            return Err(bad(format!(
                "`{cell}` is {}, but a height is a length such as `2.4mm`",
                describe(q.dim())
            )));
        }
        if (q.hi() - q.lo()).abs() > 1e-12 {
            return Err(bad(format!(
                "`{cell}` is a range, and a height is one value: write the datasheet's maximum, such as `{}mm`",
                (q.hi() * 1e12).round() / 1e9
            )));
        }
        if q.lo() < 0.0 {
            return Err(bad(format!(
                "`{cell}` is negative, and a part does not stand below the board"
            )));
        }
        Ok(Some((q.lo() * 1e12).round() / 1e9))
    }

    /// One cell as the value of a column, or `None` for an empty cell (the row has no value
    /// there and fits no design that asks for one).
    fn value(
        &self,
        catalog: &str,
        column: &str,
        kind: FieldKind,
        columns: &[(String, FieldKind)],
    ) -> Result<Option<Value>, String> {
        let Some(cell) = self.cell(column) else {
            // `derate x by y:` with nothing after it is not an empty cell, so it is not here.
            return Ok(None);
        };
        let at = format!("{}:{}", self.file, self.line);
        let bad = |why: String| format!("{at}: `{column}` of `catalog {catalog}`: {why}");
        match kind {
            // A cell of an enum column is the variant's name as it is; the solver checks it
            // against the variants when it reads the catalog for a part (ADR 0054).
            FieldKind::Text | FieldKind::Enum(_) => {
                if cell.starts_with('"') {
                    return Err(bad(format!(
                        "{cell} is written with quotes, but a text column holds its text as it is (write {})",
                        cell.trim_matches('"')
                    )));
                }
                Ok(Some(Value::Text(cell.to_string())))
            }
            FieldKind::Quantity(dim) => {
                if let Some(rest) = cell.strip_prefix("derate ") {
                    let d = self.derating(rest, dim, columns).map_err(bad)?;
                    return Ok(Some(Value::Derated(d)));
                }
                let q = quantity(cell).map_err(bad)?;
                if q.dim() != dim {
                    return Err(bad(format!(
                        "`{cell}` is {}, but this column holds {dim}",
                        describe(q.dim())
                    )));
                }
                Ok(Some(Value::Quantity(q)))
            }
        }
    }

    /// `capacitance by bias: 0:1 5:0.5`: this column is the value of the column `capacitance`
    /// of the same row, after the part keeps the fraction on the curve at the voltage across
    /// it (the part's field `bias`, ADR 0013). Nothing after the colon means that no curve
    /// is published for the part.
    fn derating(
        &self,
        text: &str,
        dim: Dimension,
        columns: &[(String, FieldKind)],
    ) -> Result<Derating, String> {
        let form = "write `derate <column> by <field>: <volts>:<fraction> ...`";
        let (head, curve) = text.split_once(':').ok_or_else(|| form.to_string())?;
        let [marked, "by", against] = head.split_whitespace().collect::<Vec<_>>()[..] else {
            return Err(form.to_string());
        };
        if !columns.iter().any(|(c, _)| c == marked) {
            return Err(format!("`{marked}` is not a column of the catalog"));
        }
        let Some(cell) = self.cell(marked) else {
            return Err(format!("this row has no `{marked}` to derate"));
        };
        let marked = quantity(cell).map_err(|e| format!("`{marked}`: {e}"))?;
        if marked.dim() != dim {
            return Err(format!(
                "`{cell}` is {}, but this column holds {dim}",
                describe(marked.dim())
            ));
        }
        Ok(Derating {
            marked,
            against: against.to_string(),
            curve: bias_curve(curve)?,
        })
    }
}

/// How a message says what a value is measured in: `in V`, or `a plain number`.
fn describe(dim: Dimension) -> String {
    if dim == Dimension::NONE {
        dim.to_string()
    } else {
        format!("in {dim}")
    }
}

/// `0:1 0.8:0.99 ...`: the fraction of a value left at each voltage. A curve starts at 0 V
/// with all of it, rises in volts and stays between 0 and 1. Nothing at all is no curve.
fn bias_curve(text: &str) -> Result<Vec<(f64, f64)>, String> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for pair in text.split_whitespace() {
        let parsed = pair
            .split_once(':')
            .and_then(|(v, f)| Some((v.parse::<f64>().ok()?, f.parse::<f64>().ok()?)));
        let Some((volts, fraction)) = parsed else {
            return Err(format!("`{pair}` in the curve is not volts:fraction"));
        };
        if !(0.0..=1.0).contains(&fraction) || points.last().is_some_and(|p| p.0 >= volts) {
            return Err(format!(
                "`{pair}` in the curve is out of order (volts must rise) or out of range (a fraction is 0 to 1)"
            ));
        }
        points.push((volts, fraction));
    }
    if !points.is_empty() && points[0] != (0.0, 1.0) {
        return Err("the curve must start at 0:1".to_string());
    }
    Ok(points)
}

/// A value in the language's syntax: a number with its unit, a range, a tolerance, or
/// arithmetic on those.
fn quantity(text: &str) -> Result<Quantity, String> {
    let e = parse_expr(text).map_err(|e| e.message)?;
    constant(&e)
}

fn constant(e: &Expr) -> Result<Quantity, String> {
    match e {
        Expr::Number(n, _) => Quantity::literal(n.value, n.unit.as_deref())
            .map(|l| l.quantity)
            .map_err(|e| e.to_string()),
        Expr::Paren(inner, _) => constant(inner),
        Expr::Neg(inner, _) => Ok(constant(inner)?.neg()),
        Expr::Binary { op, lhs, rhs, .. } => {
            let (a, b) = (constant(lhs)?, constant(rhs)?);
            match op {
                BinOp::Add => a.add(&b),
                BinOp::Sub => a.sub(&b),
                BinOp::Mul => a.mul(&b),
                BinOp::Div => a.div(&b),
            }
            .map_err(|e| e.to_string())
        }
        Expr::Range { lo, hi, .. } => {
            Quantity::range(&constant(lo)?, &constant(hi)?).map_err(|e| e.to_string())
        }
        Expr::Tolerance { center, tol, .. } => {
            let relative = matches!(
                &**tol,
                Expr::Number(n, _)
                    if n.unit.as_deref().and_then(|u| parse_unit(u).ok()).is_some_and(|u| u.relative)
            );
            Quantity::tolerance(&constant(center)?, &constant(tol)?, relative)
                .map_err(|e| e.to_string())
        }
        Expr::Str(..) => Err("a value is a number with its unit, not a quoted string".into()),
        Expr::Variant { .. } => Err(format!(
            "`{}` is a variant of an enum; a value is a number with its unit",
            e.to_source()
        )),
        Expr::Call { .. } => Err(format!(
            "`{}` is a call; a value is a number with its unit",
            e.to_source()
        )),
        Expr::Path(p) => Err(format!(
            "`{}` is a name; a value is a number with its unit (`10kohm`, `3.3V +/- 5%`, `1.8V..5.5V`)",
            p.to_source()
        )),
    }
}

/// `2026-10-31`.
fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    matches!(parts[..], [y, m, d]
        if y.len() == 4 && m.len() == 2 && d.len() == 2
            && [y, m, d].iter().all(|p| p.chars().all(|c| c.is_ascii_digit())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(rows: Vec<(usize, Vec<&str>)>) -> Vec<(usize, Vec<String>)> {
        rows.into_iter()
            .map(|(l, r)| (l, r.into_iter().map(String::from).collect()))
            .collect()
    }

    #[test]
    fn the_csv_reader_handles_quoted_commas_quotes_and_newlines_and_knows_the_lines() {
        let rows = parse_csv("a,b,c\r\n1,\"x, y\",\"say \"\"hi\"\"\"\n\n2,,\"two\nlines\"\n3,,\n")
            .unwrap();
        assert_eq!(
            rows,
            strings(vec![
                (1, vec!["a", "b", "c"]),
                (2, vec!["1", "x, y", "say \"hi\""]),
                (4, vec!["2", "", "two\nlines"]),
                (6, vec!["3", "", ""]),
            ])
        );
        let e = parse_csv("a,\"open").unwrap_err();
        assert_eq!(e, "line 1: a quoted field is never closed");
        assert_eq!(
            parse_csv("a,b,").unwrap(),
            strings(vec![(1, vec!["a", "b", ""])])
        );
    }

    #[test]
    fn values_are_numbers_with_units_ranges_tolerances_and_arithmetic() {
        let v = |t: &str| quantity(t).unwrap();
        assert_eq!(
            v("3.3V +/- 5%"),
            Quantity::tolerance(
                &Quantity::literal(3.3, Some("V")).unwrap().quantity,
                &Quantity::literal(5.0, Some("%")).unwrap().quantity,
                true
            )
            .unwrap()
        );
        assert_eq!((v("1.8V..5.5V").lo(), v("1.8V..5.5V").hi()), (1.8, 5.5));
        assert_eq!(v("-2mA").mid(), -0.002);
        assert_eq!(v("(1V + 2V) * 2").mid(), 6.0);
        assert!(quantity("1V + 2A").unwrap_err().contains("cannot add"));
        assert!(quantity("3.3V 5%").unwrap_err().contains("expected"));
    }
}
