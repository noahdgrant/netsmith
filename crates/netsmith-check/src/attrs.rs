//! Attributes (ADR 0123): `#[bank]`, `#[series(E24, E96)]`, `#[class(resistor)]`,
//! `#[value("LED")]` and `#[assembly(hand)]` on a `generic` or a `part`, and `#[role(bias)]` on a member of one. They
//! say what a definition or a member is to the checker, the solver and the tools, in a way a
//! project's own definition can say too, so that no list in Rust names a definition of the
//! standard library.
//!
//! The file pass ([`Checker::check_attributes`]) refuses an attribute that is unknown, in the
//! wrong place or with the wrong arguments, once for the definition, with "did you mean" for a
//! misspelt name. Making an instance then reads what the attributes say
//! ([`Checker::marks_of`], [`Checker::apply_roles`]) and ignores an attribute already refused.

use std::rc::Rc;

use netsmith_core::PartClass;
use netsmith_lang::ast::{AttrArg, Attribute, DefKind, Definition, Item};
use netsmith_quantity::Dimension;

use crate::checker::Checker;
use crate::diag::SourceId;
use crate::model::{FieldKind, InstId, InstKind, Member};

/// The attributes a definition may carry, with what each takes.
const DEFINITION_ATTRIBUTES: [&str; 5] = ["bank", "series", "class", "value", "assembly"];

/// The words of `#[assembly(...)]`: who assembles a part (ADR 0024), as the catalog files and
/// the manifest write it.
pub const ASSEMBLY_WORDS: [&str; 4] = ["fab", "hand", "none", "consigned"];

/// The attributes a member may carry.
const MEMBER_ATTRIBUTES: [&str; 1] = ["role"];

/// The E-series a `#[series(...)]` may name, in order from the coarsest. The solver keeps the
/// values (`netsmith-solve`, `Series`); the checker knows the names to refuse a misspelt one.
pub const SERIES_NAMES: [&str; 3] = ["E12", "E24", "E96"];

/// What a member is to the checker and the solver, whatever its name (ADR 0123, which
/// replaces the list of names of ADR 0105). A part opts in by declaring the member with the
/// role; the member then gets its value from where the role says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberRole {
    /// The voltage across a part with a `bridge`, from the rail it sits across (ADR 0013).
    Bias,
    /// The same with a sign, the first terminal of the `bridge` to the second (ADR 0088).
    BridgeVoltage,
    /// The current a part with a `bridge` passes, first terminal to second, which a `~>` chain
    /// draws from the net it starts on (ADR 0014).
    Current,
    /// The highest voltage of the rail a `~>` chain starts on (ADR 0086).
    SupplyPeak,
    /// The highest voltage the supply reaches the wrong way round, as a positive number
    /// (ADR 0091).
    SupplyReversePeak,
    /// The most current a part lets through, such as a fuse's rating (ADR 0091).
    LimitCurrent,
    /// The largest current the part in front of it that has a `limit_current` lets through
    /// (ADR 0091).
    SupplyFaultCurrent,
    /// The number of identical parts a bank stands for (ADR 0052); read only on a `#[bank]`.
    Count,
}

impl MemberRole {
    pub const ALL: [MemberRole; 8] = [
        Self::Bias,
        Self::BridgeVoltage,
        Self::Current,
        Self::SupplyPeak,
        Self::SupplyReversePeak,
        Self::LimitCurrent,
        Self::SupplyFaultCurrent,
        Self::Count,
    ];

    /// The word in `#[role(...)]`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Bias => "bias",
            Self::BridgeVoltage => "bridge_voltage",
            Self::Current => "current",
            Self::SupplyPeak => "supply_peak",
            Self::SupplyReversePeak => "supply_reverse_peak",
            Self::LimitCurrent => "limit_current",
            Self::SupplyFaultCurrent => "supply_fault_current",
            Self::Count => "count",
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.word() == word)
    }

    /// The dimension a member with the role has.
    pub fn dimension(self) -> Dimension {
        match self {
            Self::Bias | Self::BridgeVoltage | Self::SupplyPeak | Self::SupplyReversePeak => {
                Dimension::VOLTAGE
            }
            Self::Current | Self::LimitCurrent | Self::SupplyFaultCurrent => Dimension::CURRENT,
            Self::Count => Dimension::NONE,
        }
    }

    /// The type a member with the role is declared with, for a message.
    fn type_word(self) -> &'static str {
        match self.dimension() {
            d if d == Dimension::VOLTAGE => "Voltage",
            d if d == Dimension::CURRENT => "Current",
            _ => "Number",
        }
    }
}

/// What the attributes of a part's definition and of the generics it provides say (ADR 0123).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// `#[bank]`: a part of it stands for a number of identical parts in parallel, its
    /// `#[role(count)]` member (ADR 0052).
    pub bank: bool,
    /// `#[series(E24, E96)]`: the E-series its catalog's synthesized parts may take their
    /// values from (ADR 0047), by name, as written. Empty for none.
    pub series: Vec<String>,
    /// `#[class(resistor)]`: what the tools treat it as.
    pub class: Option<PartClass>,
    /// `#[value("LED")]`: what a schematic shows as its value when the solver did not choose one.
    pub value: Option<String>,
    /// `#[assembly(hand)]`: who assembles a part of it when neither the design, its catalog row
    /// nor the project says (ADR 0024), as a word of [`ASSEMBLY_WORDS`].
    pub assembly: Option<String>,
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn did_you_mean(word: &str, options: &[String]) -> String {
    crate::lower::closest(word, options)
        .map(|n| format!(" (did you mean `{n}`?)"))
        .unwrap_or_default()
}

fn list(options: &[String]) -> String {
    options
        .iter()
        .map(|o| format!("`{o}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The names of an attribute's arguments, or `None` with a message for an argument that is a
/// text.
fn name_args(
    attr: &Attribute,
) -> Result<Vec<(&str, netsmith_lang::Span)>, (netsmith_lang::Span, String)> {
    attr.args
        .iter()
        .map(|a| match a {
            AttrArg::Name(i) => Ok((i.name.as_str(), i.span)),
            AttrArg::Text { span, .. } => Err((
                *span,
                format!("`#[{}(...)]` takes names, not a text", attr.name.name),
            )),
        })
        .collect()
}

impl Checker<'_> {
    /// Refuse the attributes of a file's definitions that are unknown, misplaced or wrongly
    /// written. Each definition is read once, whether or not anything makes it.
    pub(crate) fn check_attributes(&mut self, source: SourceId, defs: &[Rc<Definition>]) {
        for def in defs {
            self.check_definition_attributes(source, def);
            let holds_roles = matches!(def.kind, DefKind::Generic | DefKind::Part);
            self.check_member_attributes(source, &def.items, holds_roles, def);
        }
    }

    fn check_definition_attributes(&mut self, source: SourceId, def: &Definition) {
        let mut seen: Vec<&str> = Vec::new();
        for attr in &def.attrs {
            let name = attr.name.name.as_str();
            if MEMBER_ATTRIBUTES.contains(&name) {
                self.report(
                    source,
                    attr.name.span,
                    format!(
                        "`#[{name}(...)]` belongs on a member of a generic or a part (`#[role(bias)] .bias: Voltage`), not on the {} `{}`",
                        def.kind.keyword(),
                        def.name.name
                    ),
                );
                continue;
            }
            if !DEFINITION_ATTRIBUTES.contains(&name) {
                let known = names(&DEFINITION_ATTRIBUTES);
                self.report(
                    source,
                    attr.name.span,
                    format!(
                        "there is no attribute `#[{name}]`{}; a generic or a part may carry {}",
                        did_you_mean(name, &known),
                        list(&known)
                    ),
                );
                continue;
            }
            if seen.contains(&name) {
                self.report(
                    source,
                    attr.span,
                    format!(
                        "`#[{name}]` is written twice on `{}`; write it once",
                        def.name.name
                    ),
                );
                continue;
            }
            seen.push(name);
            if !matches!(def.kind, DefKind::Generic | DefKind::Part) {
                self.report(
                    source,
                    attr.name.span,
                    format!(
                        "`#[{name}]` belongs on a generic or a part; `{}` is a {}",
                        def.name.name,
                        def.kind.keyword()
                    ),
                );
                continue;
            }
            match name {
                "bank" => {
                    if let Some(a) = attr.args.first() {
                        self.report(source, a.span(), "`#[bank]` takes no arguments");
                    }
                }
                "series" => self.check_series(source, def, attr),
                "class" => self.check_class(source, attr),
                "assembly" => {
                    let known = names(&ASSEMBLY_WORDS);
                    match attr.args.as_slice() {
                        [AttrArg::Name(i)] if !ASSEMBLY_WORDS.contains(&i.name.as_str()) => self
                            .report(
                                source,
                                i.span,
                                format!(
                                    "there is no assembly `{}`{}; it is one of {}",
                                    i.name,
                                    did_you_mean(&i.name, &known),
                                    list(&known)
                                ),
                            ),
                        [AttrArg::Name(_)] => {}
                        _ => self.report(
                            source,
                            attr.span,
                            format!(
                                "`#[assembly(...)]` names who assembles a part of it when nothing else says, one of {}: `#[assembly(hand)]`",
                                list(&known)
                            ),
                        ),
                    }
                }
                "value" => {
                    let ok = matches!(attr.args.as_slice(), [AttrArg::Text { .. }]);
                    if !ok {
                        self.report(
                            source,
                            attr.span,
                            "`#[value(...)]` takes one text, what a schematic shows as the part's value: `#[value(\"LED\")]`",
                        );
                    }
                }
                _ => unreachable!("the names are the list"),
            }
        }
    }

    fn check_series(&mut self, source: SourceId, def: &Definition, attr: &Attribute) {
        let args = match name_args(attr) {
            Ok(a) => a,
            Err((span, message)) => {
                self.report(source, span, message);
                return;
            }
        };
        let known = names(&SERIES_NAMES);
        if args.is_empty() {
            self.report(
                source,
                attr.span,
                format!(
                    "`#[series(...)]` names one or more E-series, such as `#[series(E24, E96)]`; the series are {}",
                    list(&known)
                ),
            );
        }
        let mut seen: Vec<&str> = Vec::new();
        for (word, span) in args {
            if !SERIES_NAMES.contains(&word) {
                self.report(
                    source,
                    span,
                    format!(
                        "there is no E-series `{word}`{}; the series are {}",
                        did_you_mean(word, &known),
                        list(&known)
                    ),
                );
            } else if seen.contains(&word) {
                self.report(
                    source,
                    span,
                    format!("`{word}` is named twice; name it once"),
                );
            }
            seen.push(word);
        }
        if !def.items.iter().any(|i| matches!(i, Item::Catalog(_))) {
            self.report(
                source,
                attr.span,
                format!(
                    "`#[series(...)]` gives values to the parts the solver makes for a catalog, and `{}` has no `catalog` statement",
                    def.name.name
                ),
            );
        }
    }

    fn check_class(&mut self, source: SourceId, attr: &Attribute) {
        let known: Vec<String> = PartClass::ALL
            .iter()
            .map(|c| c.word().to_string())
            .collect();
        let args = match name_args(attr) {
            Ok(a) => a,
            Err((span, message)) => {
                self.report(source, span, message);
                return;
            }
        };
        match args.as_slice() {
            [(word, span)] if PartClass::from_word(word).is_none() => self.report(
                source,
                *span,
                format!(
                    "there is no class `{word}`{}; the classes are {}",
                    did_you_mean(word, &known),
                    list(&known)
                ),
            ),
            [_] => {}
            _ => self.report(
                source,
                attr.span,
                format!(
                    "`#[class(...)]` names one class, such as `#[class(resistor)]`; the classes are {}",
                    list(&known)
                ),
            ),
        }
    }

    fn check_member_attributes(
        &mut self,
        source: SourceId,
        items: &[Item],
        holds_roles: bool,
        def: &Definition,
    ) {
        for item in items {
            let Item::Decl(d) = item else { continue };
            for attr in &d.attrs {
                let name = attr.name.name.as_str();
                if DEFINITION_ATTRIBUTES.contains(&name) {
                    self.report(
                        source,
                        attr.name.span,
                        format!(
                            "`#[{name}]` belongs on a generic or a part, above its definition, not on a member"
                        ),
                    );
                    continue;
                }
                if !MEMBER_ATTRIBUTES.contains(&name) {
                    let known = names(&MEMBER_ATTRIBUTES);
                    self.report(
                        source,
                        attr.name.span,
                        format!(
                            "there is no attribute `#[{name}]` for a member{}; a member may carry {}",
                            did_you_mean(name, &known),
                            list(&known)
                        ),
                    );
                    continue;
                }
                if !holds_roles {
                    self.report(
                        source,
                        attr.name.span,
                        format!(
                            "`#[role(...)]` belongs on a member of a generic or a part; `{}` is a {}",
                            def.name.name,
                            def.kind.keyword()
                        ),
                    );
                    continue;
                }
                let known: Vec<String> = MemberRole::ALL
                    .iter()
                    .map(|r| r.word().to_string())
                    .collect();
                let args = match name_args(attr) {
                    Ok(a) => a,
                    Err((span, message)) => {
                        self.report(source, span, message);
                        continue;
                    }
                };
                match args.as_slice() {
                    [(word, span)] if MemberRole::from_word(word).is_none() => self.report(
                        source,
                        *span,
                        format!(
                            "there is no role `{word}`{}; the roles are {}",
                            did_you_mean(word, &known),
                            list(&known)
                        ),
                    ),
                    [_] => {}
                    _ => self.report(
                        source,
                        attr.span,
                        format!(
                            "`#[role(...)]` names one role, such as `#[role(bias)]`; the roles are {}",
                            list(&known)
                        ),
                    ),
                }
            }
            // A member's own block (`.r: Resistor { ... }`) holds statements about it, not
            // declarations of its definition; an attribute there is in the wrong place too.
            if let Some(body) = &d.body {
                self.check_member_attributes(source, body, false, def);
            }
        }
    }

    /// What the attributes of `def` and of the generics it provides say, for an instance of it.
    /// The definition's own come first; a class or a value it does not state is the first
    /// provided generic's that does. Attributes the file pass refused are left out.
    pub(crate) fn marks_of(&self, def: &Definition, generics: &[Rc<Definition>]) -> Marks {
        let mut marks = Marks::default();
        for d in std::iter::once(def).chain(generics.iter().map(|g| g.as_ref())) {
            if !matches!(d.kind, DefKind::Generic | DefKind::Part) {
                continue;
            }
            for attr in &d.attrs {
                match attr.name.name.as_str() {
                    "bank" => marks.bank = true,
                    "series" if marks.series.is_empty() => {
                        marks.series = attr
                            .args
                            .iter()
                            .filter_map(|a| match a {
                                AttrArg::Name(i) if SERIES_NAMES.contains(&i.name.as_str()) => {
                                    Some(i.name.clone())
                                }
                                _ => None,
                            })
                            .collect();
                    }
                    "class" if marks.class.is_none() => {
                        marks.class = match attr.args.as_slice() {
                            [AttrArg::Name(i)] => PartClass::from_word(&i.name),
                            _ => None,
                        };
                    }
                    "assembly" if marks.assembly.is_none() => {
                        marks.assembly = match attr.args.as_slice() {
                            [AttrArg::Name(i)] if ASSEMBLY_WORDS.contains(&i.name.as_str()) => {
                                Some(i.name.clone())
                            }
                            _ => None,
                        };
                    }
                    "value" if marks.value.is_none() => {
                        marks.value = match attr.args.as_slice() {
                            [AttrArg::Text { text, .. }] => Some(text.clone()),
                            _ => None,
                        };
                    }
                    _ => {}
                }
            }
        }
        marks
    }

    /// Give the members `items` declared in `owner` the roles their `#[role(...)]` names. A
    /// role on a member that is not a value of the role's dimension is refused.
    pub(crate) fn apply_roles(&mut self, owner: InstId, items: &[Item]) {
        if self.model.instance(owner).kind != InstKind::Part
            && self.model.instance(owner).kind != InstKind::Module
        {
            return;
        }
        for item in items {
            let Item::Decl(d) = item else { continue };
            let [name] = d.name.segments.as_slice() else {
                continue;
            };
            for attr in &d.attrs {
                if attr.name.name != "role" {
                    continue;
                }
                let Some(role) = (match attr.args.as_slice() {
                    [AttrArg::Name(i)] => MemberRole::from_word(&i.name),
                    _ => None,
                }) else {
                    continue;
                };
                match self.model.member(owner, &name.name) {
                    Some(Member::Field(f))
                        if self.model.field(f).kind == FieldKind::Quantity(role.dimension()) =>
                    {
                        let roles = &mut self.model.fields[f.0].roles;
                        if !roles.contains(&role) {
                            roles.push(role);
                        }
                    }
                    Some(Member::Poison) | None => {}
                    Some(_) => self.error(
                        attr.span,
                        format!(
                            "`#[role({})]` is for a member of type `{}`, and `.{}` is not one",
                            role.word(),
                            role.type_word(),
                            name.name
                        ),
                    ),
                }
            }
        }
    }
}
