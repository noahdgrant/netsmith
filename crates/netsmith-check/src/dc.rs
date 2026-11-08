//! The `dc` statement: what a part is in a DC analysis (ADR 0076, amendment 1).
//!
//! A part says it once, in the part or in the generic it provides, and the checker records
//! the element on the part's instance ([`DcElement`]). The solver's DC analysis reads the
//! records (`netsmith-solve`, `dc/`).

use netsmith_lang::ast;
use netsmith_quantity::Dimension;

use crate::checker::{Checker, Res};
use crate::model::{DcElement, DcKind, FieldId, FieldKind, InstId, InstKind};

impl Checker<'_> {
    /// `dc resistor .a .b .resistance`
    pub fn dc(&mut self, d: &ast::Dc) {
        let cur = self.scope.last().copied().expect("scope is open");
        if self.model.instance(cur).kind != InstKind::Part {
            let kind = self.model.instance(cur).kind.word();
            self.error(
                d.span,
                format!(
                    "`dc` can only appear inside a part, and this is a {kind}: a module's DC network is the elements of the parts inside it"
                ),
            );
            return;
        }
        let Some(kind) = DcKind::from_word(&d.kind.name) else {
            let words: Vec<&str> = DcKind::ALL.iter().map(|k| k.word()).collect();
            let (last, init) = words.split_last().expect("there are kinds");
            self.error(
                d.kind.span,
                format!(
                    "`dc` does not know the element `{}`; the elements are {} and {last}",
                    d.kind.name,
                    init.join(", ")
                ),
            );
            return;
        };
        let pin_roles = kind.pin_roles();
        let value_roles = kind.value_roles();
        let want = pin_roles.len() + value_roles.len();
        if d.operands.len() != want {
            self.error(
                d.span,
                format!(
                    "`dc {}` takes {} pins and then {} value{}, as in `{}`; this has {} operand{}",
                    kind.word(),
                    pin_roles.len(),
                    value_roles.len(),
                    if value_roles.len() == 1 { "" } else { "s" },
                    kind.example(),
                    d.operands.len(),
                    if d.operands.len() == 1 { "" } else { "s" },
                ),
            );
            return;
        }
        let mut ok = true;
        let mut pins = Vec::new();
        for (path, role) in d.operands.iter().zip(pin_roles) {
            match self.resolve_path(path) {
                Res::Inst(i)
                    if kind.takes_rails()
                        && self.model.instance(i).kind == InstKind::Interface
                        && self.model.rail_of(i).is_some() =>
                {
                    if self.model.owner_of(i) == Some(cur) {
                        pins.push(i);
                    } else {
                        self.error(
                            path.span,
                            format!(
                                "`{}` is not a rail of this part, and `dc {}` names the rails of the part it is in (the {role})",
                                path.to_source(),
                                kind.word()
                            ),
                        );
                        ok = false;
                    }
                }
                Res::Inst(i)
                    if !kind.takes_rails() && self.model.instance(i).kind == InstKind::Pin =>
                {
                    if self.model.owner_of(i) == Some(cur) {
                        pins.push(i);
                    } else {
                        self.error(
                            path.span,
                            format!(
                                "`{}` is not a pin of this part, and `dc {}` names the pins of the part it is in (the {role})",
                                path.to_source(),
                                kind.word()
                            ),
                        );
                        ok = false;
                    }
                }
                Res::Inst(_) | Res::Field(_) => {
                    self.error(
                        path.span,
                        format!(
                            "`dc {}` takes the {role} here, {}, and `{}` is not one; the statement is `{}`",
                            kind.word(),
                            if kind.takes_rails() { "a rail" } else { "a pin" },
                            path.to_source(),
                            kind.example()
                        ),
                    );
                    ok = false;
                }
                Res::Poison | Res::Bad => ok = false,
            }
        }
        let mut values = Vec::new();
        for (path, (role, dim)) in d.operands[pin_roles.len()..].iter().zip(value_roles) {
            match self.resolve_path(path) {
                Res::Field(f) => {
                    let field = self.model.field(f);
                    if field.kind != FieldKind::Quantity(*dim) {
                        let has = match field.kind {
                            FieldKind::Quantity(d) => format!("a {}", quantity_word(d)),
                            FieldKind::Text => "text".to_string(),
                            FieldKind::Enum(_) => "a variant of an enum".to_string(),
                        };
                        self.error(
                            path.span,
                            format!(
                                "`dc {}` takes the {role} here, a {}, and `{}` is {has}",
                                kind.word(),
                                quantity_word(*dim),
                                path.to_source()
                            ),
                        );
                        ok = false;
                    } else if !self.field_of_part(f, cur) {
                        self.error(
                            path.span,
                            format!(
                                "`{}` is not a value of this part, and `dc {}` takes the {role} of the part it is in",
                                path.to_source(),
                                kind.word()
                            ),
                        );
                        ok = false;
                    } else {
                        values.push(f);
                    }
                }
                Res::Inst(_) => {
                    self.error(
                        path.span,
                        format!(
                            "`dc {}` takes the {role} here, a {}, and `{}` is not a value; the statement is `{}`",
                            kind.word(),
                            quantity_word(*dim),
                            path.to_source(),
                            kind.example()
                        ),
                    );
                    ok = false;
                }
                Res::Poison | Res::Bad => ok = false,
            }
        }
        if !ok {
            return;
        }
        if (1..pins.len()).any(|i| pins[..i].contains(&pins[i])) {
            self.error(
                d.span,
                format!(
                    "`dc {}` names the same {what} twice; its {what}s are different ones, as in `{}`",
                    kind.word(),
                    kind.example(),
                    what = if kind.takes_rails() { "rail" } else { "pin" }
                ),
            );
            return;
        }
        if let Some(earlier) = &self.model.instance(cur).dc {
            let place = self.place_of(earlier.source, earlier.span);
            self.error(
                d.span,
                format!(
                    "this part already has a `dc` statement (at {place}); a part is one element at DC"
                ),
            );
            return;
        }
        let source = self.cur_source();
        self.model.instances[cur.0].dc = Some(DcElement {
            kind,
            pins,
            values,
            source,
            span: d.span,
        });
    }

    /// Whether a field belongs to the part `part`: it is the part's own or an interface's in it.
    fn field_of_part(&self, field: FieldId, part: InstId) -> bool {
        let owner = self.model.field(field).owner;
        owner == part || self.model.owner_of(self.model.field(field).owner) == Some(part)
    }

    /// After the whole part is read: the pins a `dc` element runs between are the two its
    /// `bridge` joins, since a `~>` chain passes through the element the part is at DC.
    /// Statement order never matters, so this waits for the `bridge`.
    pub(crate) fn finish_dc(&mut self, part: InstId) {
        let inst = self.model.instance(part);
        let (Some(el), Some((a, b))) = (inst.dc.clone(), inst.bridge) else {
            return;
        };
        // The pins the element runs between: both of a two-terminal element, the drain and the
        // source of a transistor.
        let channel = match el.kind {
            DcKind::Resistor | DcKind::Diode => [el.pins[0], el.pins[1]],
            DcKind::Nmos | DcKind::Pmos => [el.pins[1], el.pins[2]],
            // A regulator is between rails, and a `bridge` is no part of it.
            DcKind::LinearRegulator | DcKind::SwitchingRegulator => return,
        };
        if (channel[0] == a && channel[1] == b) || (channel[0] == b && channel[1] == a) {
            return;
        }
        let name = |pin: InstId| format!(".{}", self.model.path_below(pin, part));
        let message = format!(
            "`dc {}` runs between {} and {}, and this part's `bridge` joins {} and {}: a `~>` chain passes through the element the part is at DC, so the two must be the same pins",
            el.kind.word(),
            name(channel[0]),
            name(channel[1]),
            name(a),
            name(b)
        );
        self.report(el.source, el.span, message);
    }
}

/// What a quantity is called in a message: `voltage`, and not the `V` of its unit.
pub(crate) fn quantity_word(dim: Dimension) -> String {
    [
        (Dimension::VOLTAGE, "voltage"),
        (Dimension::CURRENT, "current"),
        (Dimension::RESISTANCE, "resistance"),
        (Dimension::POWER, "power"),
        (Dimension::CAPACITANCE, "capacitance"),
        (Dimension::INDUCTANCE, "inductance"),
        (Dimension::FREQUENCY, "frequency"),
        (Dimension::TIME, "time"),
        (Dimension::LENGTH, "length"),
        (Dimension::NONE, "number"),
        (Dimension::TEMPERATURE, "temperature"),
    ]
    .into_iter()
    .find(|(d, _)| *d == dim)
    .map_or_else(|| format!("value in {dim}"), |(_, w)| w.to_string())
}
