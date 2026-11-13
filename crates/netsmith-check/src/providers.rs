//! Generics and their providers (ADR 0068): which parts and modules can stand where a
//! generic is used, in what order a pick tries them, and the checks that a provider has
//! what the generic states.
//!
//! # The body of a generic
//!
//! The grammar of a part's body, restricted: members `.name: Type` of any type a part may have
//! (`Pin`, quantities, text, an enum, an interface such as `.vin: Rail`) and no part or module
//! members; a `pins` table whose rows are `.pin kind` with an optional `required` or `optional`
//! and no name and no pads (stored as a row with empty `pads`: every row of a part has at least
//! one pad, so the two cannot be mistaken), or an empty table `pins { }`, which says "a part,
//! whose pins are its own" and asks for no pin (`Connector`); narrowings, relations (`==`, `>=`, `<=`), defaults
//! (`?=`), `bridge` and `.designator &= "J"`; and an optional `.mpn: String`, which a provider's
//! catalog lists as a column (not implicit, because a generic of modules has none). A relation
//! or narrowing in a generic is a check that every provider meets and every user may rely on; the
//! solver evaluates it on whichever provider is tried. All providers of one generic are parts or
//! all are modules, and a generic is placed as the kind its providers have; the mixed case is an
//! error at the later provider's own name. The kind of a pin row is a requirement on every
//! provider (it decides what the checker asks of the board); a module's pins have no kind, so a
//! generic with a `pins` table is provided by parts only, which the checker says. A generic
//! that has a pin table with pads or a `catalog` is its own first provider, and a generic with
//! neither (`CoaxReceptacle`) is useless until a provider is in scope. It holds no parts, has no
//! connections except between its own members, and is not inheritance: a use reaches only its
//! members.
//!
//! # What a provider must have (the five rules, read whenever a provider is made)
//!
//! 1. Every member and statement of each generic it provides, with the same name and type,
//!    never declared again (``` `max_frequency` is declared by `CoaxReceptacle`, which this part
//!    provides; narrow it with `.max_frequency &= ...` ```). A member that carries a block in a
//!    part (`.buck: Buck { .vref &= ... }`) becomes statements. The generic's statements
//!    are read before the provider's, each in its own file.
//! 2. Each pin of the generic has a row in the provider's table of every package, with the same
//!    kind, or is an alias of a pin that has one (`.signal ~ .rf`); an alias carries the kind of
//!    the row it is joined to, and the message names that row. A `required` or `optional` of the
//!    generic's pin reaches the provider's row (it may leave the word out; the opposite is
//!    refused); an alias does not carry the word.
//! 3. A value with another name is tied by an equation (`.max_frequency == .fmax`): no mapping
//!    block, because the two forms exist.
//! 4. The generic's narrowings and relations are added to the provider; a provider that
//!    contradicts one (`.impedance &= 75ohm` against 50 ohm) fails where it is written.
//! 5. A part that provides a part, or a module that provides an interface, is an error.
//!
//! An interface may provide a generic that has no `pins` table and no catalog (ADR 0113): a
//! manufacturing process, whose providers are its copper weights. The providers of one generic
//! are all parts, all modules or all interfaces. A contradiction on a field inside a use with
//! more than one provider is kept in `Model::provider_conflicts` as well as reported, so that
//! the build can try the other providers before it stops on it.
//!
//! A provider of a standard generic reads the generic's pins as kinds only (`.a passive`) and
//! writes the one catalog of its own; it does not declare `.package: Package`, because the
//! generic does, and it cannot state other designator letters. A provider of the standard
//! `Capacitor` is a bank. Two providers of one generic need different names, because a part is
//! known by its name in the bill of materials, in `kicad.toml` and in the notes (qualifying the
//! kind with the module path was rejected: it changes every kind name for a case the author
//! controls). A provider is judged where it is written even when no board picks it. A provider
//! that contains a use of its own generic, directly or through the parts and modules inside it,
//! would contain itself without end and is refused at the use, with the cycle named
//! (`Checker::making` keeps the stack); the same for a definition that holds itself.
//!
//! # A use
//!
//! `.name: Generic { ... }` takes the same statements as any member's block, over the generic's
//! members only. A path through a use to a member the generic does not declare is an error (the
//! module rule of ADR 0004), which is what makes the board independent of the pick; the
//! built-in members (`assembly`, `designator`, `height`, `pin_protrusion`, `model`, `place`) are exempt.
//! A module's user chooses a provider of a use inside it by narrowing the use from outside
//! (`.jack.receptacle.mpn &= "ACME-USBC-16"`, ADR 0116).
//!
//! # The candidates
//!
//! Every provider in a file the board's file imports, directly or through other files, and in
//! the board's own file: the set the loader loaded, which is the import graph
//! (`Checker::add_module` caches each file once). An import used only to bring providers in scope
//! is a plain import; the guide says to comment it. The order is the generic itself, then the
//! providers in files of the standard library, then the project's, within each by module path
//! and then name. The root file is one module whatever its spelling (`Loader::root_key`; before
//! that `src/main.nsm` and `./src/main.nsm` sorted differently). A provider's id is
//! `module::Name`, the module being the file's name without `.nsm` and with `::` for `/`, which
//! is what an `Assignment` names. A module with type parameters that provides the generic stands for
//! one candidate for each provider of its bound, `module::Buck<argmodule::Lt8641>` (ADR 0115,
//! `params.rs`). The pick depends on the sources, the catalog files and the
//! manifest only: not on KiCad libraries or the mapping, a supply provider, stock or the time,
//! so a provider with no symbol in `kicad.toml` is not skipped (the build fails at export,
//! naming it).
//!
//! # When nothing works
//!
//! No provider in scope: ``` `reg` is a `BuckConverter`, and no part or module in the files this
//! file imports provides it; import the file of a provider, or name one as the type of `reg` ```.
//! Every provider fails: ``` no BuckConverter provider satisfies reg: the closest of the 2 in
//! scope, Zbk9Buck, fails because ... ```, the closest being the trial with the fewest
//! diagnostics, ties in candidate order, the others listed after it. With one provider in scope
//! the message is the part's own.

use std::collections::HashSet;
use std::rc::Rc;

use netsmith_lang::Span;
use netsmith_lang::ast::{Decl, DefKind, Definition, Item};

use crate::checker::{Binding, Checker, Ty};
use crate::model::{GenericUse, InstId, InstKind, Member, Provider};

/// A provider with the place of its definition.
#[derive(Clone)]
pub(crate) struct ProviderRef {
    pub info: Provider,
    pub def: (usize, usize),
    /// The parts that stand for the parameters of a parametric module (`Buck<Lt8641>`, ADR
    /// 0115), in the order of its parameters. Empty for any other provider.
    pub args: Vec<(usize, usize)>,
}

/// The members every part has whatever its definition says, which a use of a generic may
/// name too.
const BUILT_IN: [&str; 9] = [
    "assembly",
    "designator",
    "height",
    "temp_min",
    "temp_max",
    "qualification",
    "pin_protrusion",
    "model",
    "place",
];

/// Whether a generic is made as a part: it has a `pins` table or a catalog. A generic with
/// neither is made as a module.
pub(crate) fn generic_is_part_shaped(def: &Definition) -> bool {
    def.items
        .iter()
        .any(|i| matches!(i, Item::Pins(_) | Item::Catalog(_)))
}

/// Whether a generic stands for itself when it has a provider-less use: it has a pin table
/// with pads or a catalog, so it is a complete part (the standard passives, ADR 0068 stage 2).
fn is_own_provider(def: &Definition) -> bool {
    def.items.iter().any(|i| match i {
        Item::Catalog(_) => true,
        Item::Pins(p) => p.rows.iter().any(|r| !r.is_contract()),
        _ => false,
    })
}

/// The word a message uses for what a definition is. A generic that is its own provider (the
/// standard passives, ADR 0068 stage 2) is placed as a part, so a message about where it may
/// stand says `part`, as it did when it was written with that keyword.
pub(crate) fn kind_word(def: &Definition) -> &'static str {
    if def.kind == DefKind::Generic && is_own_provider(def) {
        "part"
    } else {
        def.kind.keyword()
    }
}

/// The names of the members a generic declares: its values, pins and interfaces.
pub(crate) fn generic_member_names(def: &Definition) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &def.items {
        match item {
            Item::Decl(d) => {
                if let Some(first) = d.name.segments.first() {
                    names.insert(first.name.clone());
                }
            }
            Item::Pins(p) => {
                for row in &p.rows {
                    if let Some(first) = row.pin.segments.first() {
                        names.insert(first.name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

impl Checker<'_> {
    /// Every part and module in a file that this root reaches by imports (and in the root
    /// itself) whose header says it provides `generic`, in the order a pick tries them: the
    /// generic itself first when it is its own provider (it has pads or a catalog), then the
    /// providers of the standard library, then the project's, each by module path and name.
    /// Never by the order of definitions in a file (ADR 0068 section 4).
    ///
    /// A parametric module stands for one candidate for each provider of its bound (ADR 0115,
    /// `Checker::expand_parametric`), at the module's place in the order.
    pub(crate) fn providers_of(&mut self, generic: (usize, usize)) -> Vec<ProviderRef> {
        let mut out = Vec::new();
        for p in self.providers_of_unexpanded(generic) {
            let parametric = !self.modules()[p.def.0].defs[p.def.1].params.is_empty();
            if parametric && !p.info.own {
                out.extend(self.expand_parametric(&p));
            } else {
                out.push(p);
            }
        }
        out
    }

    /// [`Checker::providers_of`], with a parametric module as one entry and no arguments.
    pub(crate) fn providers_of_unexpanded(&self, generic: (usize, usize)) -> Vec<ProviderRef> {
        let gdef = &self.modules()[generic.0].defs[generic.1];
        let mut found: Vec<ProviderRef> = Vec::new();
        if is_own_provider(gdef) {
            let module = &self.modules()[generic.0];
            found.push(ProviderRef {
                info: Provider {
                    id: format!("{}::{}", module.key, gdef.name.name),
                    name: gdef.name.name.clone(),
                    module: module.key.clone(),
                    std: module.std,
                    own: true,
                    kind: InstKind::Part,
                },
                def: generic,
                args: Vec::new(),
            });
        }
        let mut others: Vec<ProviderRef> = Vec::new();
        for (m, module) in self.modules().iter().enumerate() {
            for (i, def) in module.defs.iter().enumerate() {
                let provides = def.provides.iter().any(|n| {
                    matches!(module.scope.get(&n.name), Some(Binding::Def(gm, gi)) if (*gm, *gi) == generic)
                });
                if !provides {
                    continue;
                }
                let id = format!("{}::{}", module.key, def.name.name);
                if others.iter().any(|p| p.info.id == id) {
                    continue;
                }
                others.push(ProviderRef {
                    info: Provider {
                        id,
                        name: def.name.name.clone(),
                        module: module.key.clone(),
                        std: module.std,
                        own: false,
                        kind: match def.kind {
                            DefKind::Module => InstKind::Module,
                            DefKind::Interface => InstKind::Interface,
                            _ => InstKind::Part,
                        },
                    },
                    def: (m, i),
                    args: Vec::new(),
                });
            }
        }
        others.sort_by(|a, b| {
            (!a.info.std, &a.info.module, &a.info.name).cmp(&(
                !b.info.std,
                &b.info.module,
                &b.info.name,
            ))
        });
        found.extend(others);
        found
    }

    /// The generics a definition says it provides, each with its definition. A name that is
    /// not a generic is reported (once, at the name) and left out. Read in the definition's
    /// own file.
    pub(crate) fn provided(&mut self, def: &Definition) -> Vec<(Rc<Definition>, (usize, usize))> {
        let mut found = Vec::new();
        for name in &def.provides {
            let tp = netsmith_lang::ast::TypePath {
                segments: vec![name.clone()],
                args: Vec::new(),
                of_member: false,
            };
            match self.resolve_type(&tp) {
                Ty::Def(m, i) => {
                    let g = self.modules()[m].defs[i].clone();
                    if g.kind == DefKind::Generic {
                        if !found.iter().any(|(_, r)| *r == (m, i)) {
                            found.push((g, (m, i)));
                        }
                    } else {
                        self.error(
                            name.span,
                            format!(
                                "`{}` is {} {}, and only a `generic` is provided; `{}` provides nothing until `{}` is declared with `generic {}`",
                                g.name.name,
                                if g.kind == DefKind::Interface { "an" } else { "a" },
                                g.kind.keyword(),
                                def.name.name,
                                g.name.name,
                                g.name.name
                            ),
                        );
                    }
                }
                Ty::Enum(..) => self.error(
                    name.span,
                    format!(
                        "`{}` is an enum, and only a `generic` is provided",
                        name.name
                    ),
                ),
                Ty::Builtin(_) => self.error(
                    name.span,
                    format!(
                        "`{}` is a built-in type, and only a `generic` is provided",
                        name.name
                    ),
                ),
                Ty::Param { .. } => self.error(
                    name.span,
                    format!(
                        "`{}` is a type parameter, and only a `generic` is provided",
                        name.name
                    ),
                ),
                Ty::Poison | Ty::Bad => {}
            }
        }
        found
    }

    /// The items of a generic as a provider reads them. A module has no pins with kinds, so
    /// a generic's `pins` table is not read into one (and `conform` says so).
    ///
    /// A generic that is its own provider (the standard passives, ADR 0068 stage 2) has pads
    /// and a catalog that belong to it alone: a provider reads its `pins` tables as one table
    /// of kinds with no names and no pads, which is what a provider's own rows must agree
    /// with, and not its `catalog`.
    pub(crate) fn generic_items(
        &mut self,
        generic: &Rc<Definition>,
        provider: &Definition,
        provider_kind: InstKind,
    ) -> Vec<Item> {
        let _ = provider;
        let own = is_own_provider(generic);
        let mut items = Vec::new();
        let mut contract: Option<usize> = None;
        for item in &generic.items {
            match item {
                Item::Pins(_) if provider_kind != InstKind::Part => {}
                Item::Catalog(_) if provider_kind == InstKind::Interface => {}
                Item::Catalog(_) if own => {}
                // A generic that holds one is refused where it is checked on its own; a
                // provider does not take it in (ADR 0071 section 6).
                Item::Peripheral(_) | Item::Shared(_) | Item::Lines(_) => {}
                Item::Pins(table) if own => {
                    let at = *contract.get_or_insert_with(|| {
                        items.push(Item::Pins(netsmith_lang::ast::Pins {
                            package: None,
                            rows: Vec::new(),
                            span: table.span,
                        }));
                        items.len() - 1
                    });
                    let Item::Pins(merged) = &mut items[at] else {
                        continue;
                    };
                    for row in &table.rows {
                        let pin = row.pin.to_source();
                        if merged.rows.iter().any(|r| r.pin.to_source() == pin) {
                            continue;
                        }
                        let mut row = row.clone();
                        row.name = String::new();
                        row.pads = Vec::new();
                        row.functions = Vec::new();
                        merged.rows.push(row);
                    }
                }
                other => items.push(other.clone()),
            }
        }
        items
    }

    /// A member of a provider that the generic declared already, or one the provider
    /// declared twice: the message for it.
    pub(crate) fn redeclared(&self, owner: InstId, name: &str, d: &Decl) -> String {
        let Some(generic) = self.generic_members.get(&(owner, name.to_string())) else {
            return format!("`{name}` is already declared in this block");
        };
        let word = match self.model.instance(owner).kind {
            InstKind::Module => "module",
            _ => "part",
        };
        let how = match self.model.member(owner, name) {
            Some(Member::Field(_)) if d.body.is_none() => {
                format!("narrow it with `.{name} &= ...`, or state it with a relation")
            }
            _ => format!("state its values with statements, `.{name}.<field> &= ...`"),
        };
        format!(
            "`{name}` is declared by `{generic}`, which this {word} provides; do not declare it again: {how}"
        )
    }

    /// Whether `inst`, made as a provider, may name `member` from the definition being read:
    /// a use reaches only what the generic declares (and what every part has).
    pub(crate) fn outside_contract(&self, inst: InstId, name: &str) -> Option<String> {
        let instance = self.model.instance(inst);
        let generic = instance.placed_def?;
        if instance.def == self.cur_def_ref() || Some(generic) == self.cur_def_ref() {
            return None;
        }
        // The statements of another generic that the same provider provides are read into the
        // instance too.
        if let Some((m, i)) = self.cur_def_ref() {
            let reading = &self.modules()[m].defs[i];
            if reading.kind == DefKind::Generic && instance.provides.contains(&reading.name.name) {
                return None;
            }
        }
        if BUILT_IN.contains(&name) {
            return None;
        }
        let gdef = &self.modules()[generic.0].defs[generic.1];
        if generic_member_names(gdef).contains(name) {
            return None;
        }
        Some(gdef.name.name.clone())
    }

    /// A use of a generic with no provider in scope: reported at the line that declares it,
    /// once the declaration is known.
    pub(crate) fn no_provider(&mut self, child: InstId, d: &Decl) {
        let Some(u) = self
            .uses
            .iter()
            .find(|u| u.inst == child && u.candidates.is_empty())
        else {
            return;
        };
        let (member, generic) = (u.path.clone(), u.generic.clone());
        let at = self.model.instance(child).declared;
        let (source, span) = at.unwrap_or((self.cur_source(), d.span));
        self.report(
            source,
            span,
            format!(
                "`{member}` is a `{generic}`, and no part or module in the files this file imports provides it; import the file of a provider, or name one as the type of `{member}` (a part or module declared with `provides {generic}`)"
            ),
        );
    }

    /// A member declared with a generic's type: made from the provider the assignment names,
    /// else the first in scope; the generic itself when it has no provider.
    pub(crate) fn instantiate_use(
        &mut self,
        generic: &Rc<Definition>,
        generic_ref: (usize, usize),
        name: &str,
        parent: InstId,
    ) -> Option<InstId> {
        let providers = self.providers_of(generic_ref);
        self.check_provider_kinds(generic_ref);
        // Whatever the order of the providers, a pick that can take a definition that is
        // being made here would make it again inside itself, so the use is refused, not the
        // first provider that happens to be another one.
        let again = providers
            .iter()
            .map(|p| p.def)
            .chain(providers.is_empty().then_some(generic_ref))
            .find(|d| self.making.contains(d));
        if let Some(again) = again {
            self.report_cycle(name, Some(&generic.name.name), again);
            return None;
        }
        let path = match self.model.path_of(parent) {
            up if up.is_empty() => name.to_string(),
            up => format!("{up}.{name}"),
        };
        let chosen = self
            .assign
            .get(&path)
            .and_then(|id| providers.iter().position(|p| &p.info.id == id))
            .unwrap_or(0);
        let slot = self.uses.len();
        self.uses.push(GenericUse {
            path,
            inst: InstId(0),
            generic: generic.name.name.clone(),
            candidates: providers.iter().map(|p| p.info.clone()).collect(),
            chosen,
        });
        let id = match providers.get(chosen) {
            // The generic is its own provider: it is made as it is written.
            Some(p) if p.info.own => {
                self.instantiate_as(generic, generic_ref, name, Some(parent), None)
            }
            Some(p) => {
                let def = self.modules()[p.def.0].defs[p.def.1].clone();
                if !p.args.is_empty() {
                    self.next_args = Some(p.args.clone());
                }
                self.instantiate_as(
                    &def,
                    p.def,
                    name,
                    Some(parent),
                    Some((generic_ref, generic.name.name.clone())),
                )
            }
            None => self.instantiate_as(generic, generic_ref, name, Some(parent), None),
        };
        self.uses[slot].inst = id;
        Some(id)
    }

    /// A definition that would be made inside itself: the member, the cycle of definitions
    /// from the one made again to the member, and what to do, at the member's declaration.
    pub(crate) fn report_cycle(
        &mut self,
        member: &str,
        generic: Option<&str>,
        again: (usize, usize),
    ) {
        let name_of =
            |this: &Self, d: (usize, usize)| this.modules()[d.0].defs[d.1].name.name.clone();
        let from = self.making.iter().position(|d| *d == again).unwrap_or(0);
        let mut chain: Vec<String> = self.making[from..]
            .iter()
            .map(|d| format!("`{}`", name_of(self, *d)))
            .collect();
        chain.push(format!("`{}`", name_of(self, again)));
        let chain = chain.join(" > ");
        let held = name_of(self, again);
        let message = match generic {
            Some(generic) => format!(
                "`{member}` is a `{generic}`, and `{held}`, which is being made here, can be the provider picked for it, so `{held}` would contain itself without end (the cycle: {chain}); a provider cannot contain a use of the generic it provides, directly or through the parts and modules inside it: wrap a part, or a module that does not provide `{generic}`"
            ),
            None => format!(
                "`{member}` would make `{held}` inside itself without end (the cycle: {chain}); a definition cannot contain itself, directly or through the parts and modules inside it"
            ),
        };
        let (source, span) = self.use_site.unwrap_or((
            self.cur_source(),
            self.modules()[again.0].defs[again.1].span,
        ));
        self.report(source, span, message);
    }

    /// What a provider owes the generics it provides, after both have been read into the
    /// instance (ADR 0068 section 3): a pin row of the generic has a row of the same kind in
    /// every table of the part (or is an alias of a pin that has one), and the providers of
    /// one generic are all parts, all modules or all interfaces.
    pub(crate) fn conform(
        &mut self,
        inst: InstId,
        def: &Definition,
        generics: &[(Rc<Definition>, (usize, usize))],
    ) {
        let kind = self.model.instance(inst).kind;
        for (g, gref) in generics {
            self.check_provider_kinds(*gref);
            if kind == InstKind::Module && g.items.iter().any(|i| matches!(i, Item::Pins(_))) {
                self.error(
                    def.name.span,
                    format!(
                        "`{}` is a module, and `{}` has a `pins` table, which only a part provides: a module's pins have no kinds to compare",
                        def.name.name, g.name.name
                    ),
                );
            }
            if kind == InstKind::Interface && generic_is_part_shaped(g) {
                self.error(
                    def.name.span,
                    format!(
                        "`{}` is an interface, and `{}` has a `pins` table or a catalog, which only a part provides: an interface provides a generic of values and interfaces only, such as a manufacturing process",
                        def.name.name, g.name.name
                    ),
                );
            }
        }
        if kind != InstKind::Part {
            return;
        }
        let rows: Vec<_> = self
            .contract_rows
            .iter()
            .filter(|r| r.inst == inst)
            .cloned()
            .collect();
        for r in rows {
            let pin_name = self.model.instance(r.pin).name.clone();
            let tables = self.model.instance(inst).tables.clone();
            if tables.is_empty() {
                self.report_at_name(
                    def,
                    format!(
                        "`{}` provides `{}`, which has the pin `.{pin_name}`, and this part has no `pins` table to give it a pad; add `.{pin_name} {} \"NAME\" \"pad\"` to a `pins` table",
                        def.name.name,
                        r.generic,
                        r.kind.word()
                    ),
                );
                continue;
            }
            for (ti, t) in tables.iter().enumerate() {
                let own_row = t
                    .row(r.pin)
                    .map(|row| (row.kind, row.requirement, row.span));
                // An alias: the pin is joined inside the part to one that has a row here, and
                // that row is what the checker reads for the net, so its kind is compared.
                let (row, alias) = match own_row {
                    Some(row) => (Some(row), None),
                    None => {
                        let joined = self.model.joined_inside(r.pin).into_iter().find_map(|p| {
                            t.row(p)
                                .map(|row| (p, (row.kind, row.requirement, row.span)))
                        });
                        match joined {
                            Some((p, row)) => {
                                (Some(row), Some(self.model.instance(p).name.clone()))
                            }
                            None => (None, None),
                        }
                    }
                };
                let table = t.describe();
                match row {
                    None => self.report(
                        t.source,
                        t.span,
                        format!(
                            "`{table}` of `{}` has no row for `.{pin_name}`, which `{}` declares as {}; add `.{pin_name} {} \"NAME\" \"pad\"`, or join it to a pin that has a row (`.{pin_name} ~ .other`)",
                            def.name.name,
                            r.generic,
                            crate::pins::article(r.kind),
                            r.kind.word()
                        ),
                    ),
                    Some((kind, requirement, span)) => {
                        if kind != r.kind {
                            let row = match &alias {
                                Some(joined) => format!(
                                    "the row of `.{joined}`, which `.{pin_name}` is joined to,"
                                ),
                                None => "this row".to_string(),
                            };
                            self.report(
                                t.source,
                                span,
                                format!(
                                    "`.{pin_name}` is {} in `{}`, which `{}` provides, but {row} says {}; a provider's row has the generic's kind, because the kind decides what the checker asks of the board",
                                    crate::pins::article(r.kind),
                                    r.generic,
                                    def.name.name,
                                    crate::pins::article(kind),
                                ),
                            );
                        } else if let (Some(wanted), Some(given)) = (r.requirement, requirement)
                            && wanted != given
                        {
                            self.report(
                                t.source,
                                span,
                                format!(
                                    "`.{pin_name}` is `{}` in `{}`, which `{}` provides, but this row says `{}`",
                                    wanted.word(),
                                    r.generic,
                                    def.name.name,
                                    given.word()
                                ),
                            );
                        } else if requirement.is_none()
                            && r.requirement.is_some()
                            && t.row(r.pin).is_some()
                        {
                            // The generic's `required` or `optional` holds for the row.
                            if let Some(row) = self.model.instances[inst.0].tables[ti]
                                .rows
                                .iter_mut()
                                .find(|row| row.pin == r.pin)
                            {
                                row.requirement = r.requirement;
                            }
                        }
                    }
                }
            }
        }
    }

    /// All the providers of a generic are parts, or all are modules (ADR 0068): a provider of
    /// the other kind than the first is reported at its own name, in its own file.
    pub(crate) fn check_provider_kinds(&mut self, generic: (usize, usize)) {
        let providers = self.providers_of_unexpanded(generic);
        let generic_name = self.modules()[generic.0].defs[generic.1].name.name.clone();
        self.check_provider_names(&providers, &generic_name);
        let Some(first) = providers.iter().find(|p| !p.info.own) else {
            return;
        };
        let (first_name, first_kind) = (first.info.name.clone(), first.info.kind);
        let word = |kind: InstKind| match kind {
            InstKind::Module => "a module",
            InstKind::Interface => "an interface",
            _ => "a part",
        };
        for p in providers.iter().filter(|p| !p.info.own) {
            if p.info.kind == first_kind {
                continue;
            }
            let (source, span) = {
                let def = &self.modules()[p.def.0].defs[p.def.1];
                (self.modules()[p.def.0].source, def.name.span)
            };
            self.report(
                source,
                span,
                format!(
                    "`{}` is {}, but `{first_name}`, which provides `{generic_name}` too, is {}; all providers of a generic are parts, all are modules, or all are interfaces, because a generic is placed as the kind its providers have",
                    p.info.name,
                    word(p.info.kind),
                    word(first_kind),
                ),
            );
        }
    }

    /// No two providers of one generic have the same name (ADR 0068): a part is known by its
    /// name in the bill of materials, in `kicad.toml`'s `[[part]]` entries and in the notes, so
    /// two of them would share a kind, a mapping and a line. The later one is reported, at its
    /// own name in its own file, with both module paths.
    fn check_provider_names(&mut self, providers: &[ProviderRef], generic: &str) {
        for (at, p) in providers.iter().enumerate() {
            let Some(first) = providers[..at].iter().find(|q| q.info.name == p.info.name) else {
                continue;
            };
            let (source, span) = {
                let def = &self.modules()[p.def.0].defs[p.def.1];
                (self.modules()[p.def.0].source, def.name.span)
            };
            let first_is = if first.info.own {
                format!("the generic itself (`{}`)", first.info.id)
            } else {
                format!("`{}`", first.info.id)
            };
            self.report(
                source,
                span,
                format!(
                    "`{}` is a provider of `{generic}` in `{}` and is also one in {first_is}; two providers of one generic need different names, because a part is known by its name in the bill of materials, in `kicad.toml` and in the notes: rename one",
                    p.info.name, p.info.module
                ),
            );
        }
    }

    fn report_at_name(&mut self, def: &Definition, message: String) {
        self.error(def.name.span, message);
    }

    pub(crate) fn cur_def_ref(&self) -> Option<(usize, usize)> {
        self.cur_def
    }

    /// The span of a use's declaration, for a message that is about the use.
    #[allow(dead_code)]
    pub(crate) fn use_span(&self, inst: InstId) -> Option<Span> {
        self.model.instance(inst).declared.map(|(_, s)| s)
    }
}
