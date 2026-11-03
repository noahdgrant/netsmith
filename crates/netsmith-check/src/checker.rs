//! Loading modules, resolving imports and names, and elaborating definitions into a
//! [`Model`].

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use netsmith_lang::ast::{Decl, DefKind, Definition, Import, Item, Path, TypePath};
use netsmith_lang::{Span, parse};
use netsmith_quantity::Dimension;

use crate::diag::{Diagnostic, SourceId, Sources};
use crate::loader::Loader;
use crate::model::{FieldKind, InstId, InstKind, Instance, Member, Model};

#[derive(Clone, Copy)]
pub(crate) enum Binding {
    Def(usize, usize),
    /// An import that failed. Uses of the name are not reported again.
    Poison,
}

pub(crate) struct ModuleData {
    pub source: SourceId,
    pub defs: Vec<Rc<Definition>>,
    pub scope: HashMap<String, Binding>,
}

#[derive(Clone, Copy)]
pub(crate) enum Builtin {
    Quantity(Dimension),
    Text,
    Pin,
}

pub(crate) enum Ty {
    Builtin(Builtin),
    Def(usize, usize),
    Poison,
    Bad,
}

/// What a path resolved to.
pub(crate) enum Res {
    Inst(InstId),
    Field(crate::model::FieldId),
    Poison,
    /// Already reported.
    Bad,
}

pub(crate) struct Checker<'a> {
    loader: &'a Loader,
    pub sources: Sources,
    modules: Vec<ModuleData>,
    cache: HashMap<String, usize>,
    diags: Vec<Diagnostic>,
    seen: HashSet<Diagnostic>,
    // State of the elaboration in progress.
    pub model: Model,
    pub scope: Vec<InstId>,
    pub cur_module: usize,
    /// The definition whose source text is being checked. Anything else is "outside".
    cur_def: Option<(usize, usize)>,
    pub uf: HashMap<usize, usize>,
    skipped: HashSet<(usize, usize)>,
    /// Pins that a row of a `pins` table declared, as opposed to a `.name: Pin` member.
    pub(crate) table_pins: HashSet<InstId>,
}

fn builtin(name: &str) -> Option<Builtin> {
    use Dimension as D;
    let q = |d| Some(Builtin::Quantity(d));
    match name {
        "Pin" => Some(Builtin::Pin),
        "String" | "Package" | "Color" => Some(Builtin::Text),
        "Number" => q(D::NONE),
        "Voltage" => q(D::VOLTAGE),
        "Current" => q(D::CURRENT),
        "Resistance" => q(D::RESISTANCE),
        "Capacitance" => q(D::CAPACITANCE),
        "Inductance" => q(D::INDUCTANCE),
        "Frequency" => q(D::FREQUENCY),
        "Time" => q(D::TIME),
        "Length" => q(D::LENGTH),
        "Power" => q(D::POWER),
        "Charge" => q(D::CHARGE),
        "Temperature" => q(D::TEMPERATURE),
        _ => None,
    }
}

impl<'a> Checker<'a> {
    pub fn new(loader: &'a Loader) -> Self {
        Self {
            loader,
            sources: Sources::default(),
            modules: Vec::new(),
            cache: HashMap::new(),
            diags: Vec::new(),
            seen: HashSet::new(),
            model: Model::new(),
            scope: Vec::new(),
            cur_module: 0,
            cur_def: None,
            uf: HashMap::new(),
            skipped: HashSet::new(),
            table_pins: HashSet::new(),
        }
    }

    pub fn into_diagnostics(self) -> (Vec<Diagnostic>, Sources) {
        (self.diags, self.sources)
    }

    pub fn module_defs(&self, module: usize) -> Vec<Rc<Definition>> {
        self.modules[module].defs.clone()
    }

    pub fn report(&mut self, source: SourceId, span: Span, message: impl Into<String>) {
        let d = Diagnostic {
            source,
            span,
            message: message.into(),
        };
        if self.seen.insert(d.clone()) {
            self.diags.push(d);
        }
    }

    /// Add diagnostics found outside the checker proper.
    pub fn extend(&mut self, diagnostics: Vec<Diagnostic>) {
        for d in diagnostics {
            self.report(d.source, d.span, d.message);
        }
    }

    /// Whether the definition being read is a `board`.
    pub fn in_board(&self) -> bool {
        self.in_kind(DefKind::Board)
    }

    /// Whether the definition being read is a `system`.
    pub fn in_system(&self) -> bool {
        self.in_kind(DefKind::System)
    }

    /// Whether the definition being read is a `panel`.
    pub fn in_panel(&self) -> bool {
        self.in_kind(DefKind::Panel)
    }

    fn in_kind(&self, kind: DefKind) -> bool {
        self.cur_def
            .is_some_and(|(m, i)| self.modules[m].defs[i].kind == kind)
    }

    pub fn cur_source(&self) -> SourceId {
        self.modules[self.cur_module].source
    }

    pub fn error(&mut self, span: Span, message: impl Into<String>) {
        let source = self.cur_source();
        self.report(source, span, message);
    }

    // ---- modules and imports -------------------------------------------------

    pub fn add_root(&mut self, name: &str, text: &str) -> usize {
        self.add_module(name, text, None)
    }

    fn load_module(&mut self, path: &[String]) -> Option<usize> {
        let key = path.join("::");
        if let Some(&id) = self.cache.get(&key) {
            return Some(id);
        }
        let (file_name, text) = self.loader.get(&key)?.clone();
        Some(self.add_module(&file_name, &text, Some(key)))
    }

    fn add_module(&mut self, name: &str, text: &str, key: Option<String>) -> usize {
        let source = self.sources.add(name, text);
        let mut file = match parse(text) {
            Ok(file) => file,
            Err(e) => {
                self.report(source, e.span, e.message);
                netsmith_lang::ast::File {
                    inner_docs: Vec::new(),
                    imports: Vec::new(),
                    definitions: Vec::new(),
                }
            }
        };
        let imports = std::mem::take(&mut file.imports);
        let defs: Vec<Rc<Definition>> = file.definitions.into_iter().map(Rc::new).collect();

        let id = self.modules.len();
        let mut scope = HashMap::new();
        for (i, def) in defs.iter().enumerate() {
            if scope
                .insert(def.name.name.clone(), Binding::Def(id, i))
                .is_some()
            {
                self.report(
                    source,
                    def.name.span,
                    format!("`{}` is defined twice in this file", def.name.name),
                );
            }
        }
        self.modules.push(ModuleData {
            source,
            defs,
            scope,
        });
        // Cached before the imports run so a cycle cannot recurse forever.
        if let Some(key) = key {
            self.cache.insert(key, id);
        }
        for import in &imports {
            self.apply_import(id, import);
        }
        id
    }

    fn apply_import(&mut self, module: usize, import: &Import) {
        let source = self.modules[module].source;
        let segments: Vec<String> = import.path.iter().map(|i| i.name.clone()).collect();
        let (module_path, names): (Vec<String>, Vec<(String, Span)>) = match &import.names {
            Some(list) => (
                segments,
                list.iter().map(|i| (i.name.clone(), i.span)).collect(),
            ),
            None => {
                let (last, init) = segments.split_last().expect("a path has a segment");
                let span = import.path.last().expect("a path has a segment").span;
                (init.to_vec(), vec![(last.clone(), span)])
            }
        };

        let target = if module_path.is_empty() {
            self.report(
                source,
                import.span,
                "an import needs a module path before the name",
            );
            None
        } else {
            let found = self.load_module(&module_path);
            if found.is_none() {
                self.report(
                    source,
                    import.span,
                    format!("unknown module `{}`", module_path.join("::")),
                );
            }
            found
        };

        for (name, span) in names {
            let binding = match target {
                None => Binding::Poison,
                Some(t) => match self.modules[t]
                    .defs
                    .iter()
                    .position(|d| d.name.name == name)
                {
                    Some(index) => Binding::Def(t, index),
                    None => {
                        self.report(
                            source,
                            span,
                            format!(
                                "module `{}` has no definition `{name}`",
                                module_path.join("::")
                            ),
                        );
                        Binding::Poison
                    }
                },
            };
            if self.modules[module]
                .scope
                .insert(name.clone(), binding)
                .is_some()
            {
                self.report(
                    source,
                    span,
                    format!("`{name}` is already defined or imported"),
                );
            }
        }
    }

    pub fn resolve_type(&mut self, tp: &TypePath) -> Ty {
        let first = &tp.segments[0];
        let bound = self.modules[self.cur_module]
            .scope
            .get(&first.name)
            .copied();
        if tp.segments.len() > 1 {
            if matches!(bound, Some(Binding::Poison)) {
                return Ty::Poison;
            }
            let span = Span::new(
                first.span.start,
                tp.segments.last().expect("non-empty").span.end,
            );
            self.error(
                span,
                "qualified type names such as `A::B` are not supported yet",
            );
            return Ty::Bad;
        }
        match bound {
            Some(Binding::Def(m, i)) => Ty::Def(m, i),
            Some(Binding::Poison) => Ty::Poison,
            None => match builtin(&first.name) {
                Some(b) => Ty::Builtin(b),
                None => {
                    self.error(first.span, format!("unknown type `{}`", first.name));
                    Ty::Bad
                }
            },
        }
    }

    // ---- elaboration ---------------------------------------------------------

    /// Build a fresh model of one definition, with the checks that need the whole tree.
    pub fn elaborate(&mut self, module: usize, index: usize) -> Model {
        self.model = Model::new();
        self.scope.clear();
        self.uf.clear();
        self.skipped.clear();
        self.table_pins.clear();
        let def = self.modules[module].defs[index].clone();
        let name = def.name.name.clone();
        let root = self.instantiate(&def, (module, index), &name, None);
        self.model.root = root;
        self.finalize_nets();
        std::mem::replace(&mut self.model, Model::new())
    }

    fn instantiate(
        &mut self,
        def: &Rc<Definition>,
        def_ref: (usize, usize),
        name: &str,
        parent: Option<InstId>,
    ) -> InstId {
        let kind = match def.kind {
            DefKind::Board => InstKind::Board,
            DefKind::System => InstKind::System,
            DefKind::Panel => InstKind::Panel,
            DefKind::Module => InstKind::Module,
            DefKind::Part => InstKind::Part,
            DefKind::Interface => InstKind::Interface,
        };
        let id = self.model.add_instance(Instance {
            name: name.to_string(),
            kind,
            type_name: def.name.name.clone(),
            parent,
            children: Vec::new(),
            bridge: None,
            catalog: None,
            tables: Vec::new(),
            ties: Vec::new(),
            source: self.modules[def_ref.0].source,
            span: def.span,
            declared: None,
            docs: def.docs.clone(),
            def: Some(def_ref),
        });
        // Every part has a field for who assembles it (ADR 0024): the board narrows it, the
        // catalog row may state a default, and lowering reads it. It has one for the letters
        // of its reference designator too (ADR 0046): its kind states them, a user part or a
        // board may narrow it, and lowering reads it.
        if kind == InstKind::Part {
            for name in ["assembly", "designator"] {
                let field = self.model.add_field(id, name, FieldKind::Text);
                self.model.add_member(id, name, Member::Field(field));
            }
            // And one for how tall its body stands above the board (ADR 0045, "Height
            // limits", PROVISIONAL): the part or the board states it, and lowering reads it.
            // A part that does not has no height, which is not a height of zero.
            let field = self
                .model
                .add_field(id, "height", FieldKind::Quantity(Dimension::LENGTH));
            self.model.add_member(id, "height", Member::Field(field));
        }
        // Every part and module has a `.place` for the requirements on where it goes on the
        // board (ADR 0045): the board narrows its fields like any others.
        if matches!(kind, InstKind::Part | InstKind::Module) {
            self.add_place(id, def.span);
        }
        // A definition's names are resolved inside that definition's own file, and bare
        // names look outward only through its own blocks, never through its user.
        let saved_module = std::mem::replace(&mut self.cur_module, def_ref.0);
        let saved_def = self.cur_def.replace(def_ref);
        let saved_scope = std::mem::replace(&mut self.scope, vec![id]);
        self.declare(&def.items);
        self.process(&def.items);
        self.cur_module = saved_module;
        self.cur_def = saved_def;
        self.scope = saved_scope;
        id
    }

    /// The built-in `.place` of a part or module (ADR 0045): an interface `Place` with the
    /// box of the body in the board's frame (`x` and `y` of its centre, `left`, `right`,
    /// `top` and `bottom` of its edges), the turn in degrees and the face. A module's has
    /// no `rotation` and no `side`: the placer does not turn a module as a block.
    fn add_place(&mut self, owner: InstId, span: Span) {
        let source = self.model.instance(owner).source;
        let place = self.model.add_instance(Instance {
            name: "place".to_string(),
            kind: InstKind::Interface,
            type_name: "Place".to_string(),
            parent: Some(owner),
            children: Vec::new(),
            bridge: None,
            catalog: None,
            tables: Vec::new(),
            ties: Vec::new(),
            source,
            span,
            declared: None,
            docs: Vec::new(),
            def: None,
        });
        let module = self.model.instance(owner).kind == InstKind::Module;
        let fields = [
            ("x", FieldKind::Quantity(Dimension::LENGTH), false),
            ("y", FieldKind::Quantity(Dimension::LENGTH), false),
            ("left", FieldKind::Quantity(Dimension::LENGTH), false),
            ("right", FieldKind::Quantity(Dimension::LENGTH), false),
            ("top", FieldKind::Quantity(Dimension::LENGTH), false),
            ("bottom", FieldKind::Quantity(Dimension::LENGTH), false),
            ("rotation", FieldKind::Quantity(Dimension::NONE), true),
            ("side", FieldKind::Text, true),
        ];
        for (name, kind, part_only) in fields {
            if part_only && module {
                continue;
            }
            let field = self.model.add_field(place, name, kind);
            self.model.add_member(place, name, Member::Field(field));
        }
        self.model.add_member(owner, "place", Member::Inst(place));
    }

    pub(crate) fn current(&self) -> InstId {
        *self
            .scope
            .last()
            .expect("a scope is always open during elaboration")
    }

    /// First pass: create every member, so later statements can name any of them.
    fn declare(&mut self, items: &[Item]) {
        for item in items {
            if let Item::Pins(table) = item {
                self.declare_table_pins(table);
                continue;
            }
            let Item::Decl(d) = item else { continue };
            let cur = self.current();
            let key = (self.cur_source().0, d.span.start);

            if !(d.name.leading_dot && d.name.segments.len() == 1) {
                self.error(
                    d.name.span,
                    "a declaration needs a single member name such as `.name`",
                );
                self.skipped.insert(key);
                continue;
            }
            let name = d.name.segments[0].name.clone();
            if self.model.member(cur, &name).is_some() {
                self.error(
                    d.name.span,
                    format!("`{name}` is already declared in this block"),
                );
                self.skipped.insert(key);
                continue;
            }

            let ty = self.resolve_type(&d.ty);
            if (self.in_system() || self.in_panel()) && matches!(ty, Ty::Builtin(_)) {
                let holder = self.model.instance(self.current()).kind.word();
                self.error(
                    d.ty.segments[0].span,
                    format!(
                        "a {holder} holds boards, but `{}` is not one; put values and pins on a board",
                        d.ty.segments[0].name
                    ),
                );
                self.model.add_member(cur, &name, Member::Poison);
                continue;
            }
            match ty {
                Ty::Poison | Ty::Bad => self.model.add_member(cur, &name, Member::Poison),
                Ty::Builtin(Builtin::Quantity(dim)) => self.add_scalar(
                    cur,
                    &name,
                    FieldKind::Quantity(dim),
                    d.body.is_some(),
                    d.span,
                ),
                Ty::Builtin(Builtin::Text) => {
                    self.add_scalar(cur, &name, FieldKind::Text, d.body.is_some(), d.span)
                }
                Ty::Builtin(Builtin::Pin) => {
                    let pin = self.add_pin(cur, &name, d.span, d.docs.clone());
                    self.model.add_member(cur, &name, Member::Inst(pin));
                }
                Ty::Def(m, i) if self.misplaced(&self.modules[m].defs[i].clone(), d) => {
                    self.model.add_member(cur, &name, Member::Poison)
                }
                Ty::Def(m, i) => {
                    let def = self.modules[m].defs[i].clone();
                    let child = self.instantiate(&def, (m, i), &name, Some(cur));
                    self.model.instances[child.0].docs = d.docs.clone();
                    let last = d.ty.segments.last().map_or(d.name.span.end, |s| s.span.end);
                    self.model.instances[child.0].declared =
                        Some((self.cur_source(), Span::new(d.name.span.start, last)));
                    self.model.add_member(cur, &name, Member::Inst(child));
                }
            }

            if let (Some(body), Some(Member::Inst(child))) =
                (&d.body, self.model.member(cur, &name))
            {
                self.scope.push(child);
                self.declare(body);
                self.scope.pop();
            }
        }
    }

    /// Whether the panel being read has already declared its board.
    fn panel_has_cell(&self) -> bool {
        self.model
            .instance(self.current())
            .children
            .iter()
            .any(|(_, m)| matches!(m, Member::Inst(i) if self.model.instance(*i).kind == InstKind::Board))
    }

    /// Whether a declaration puts the wrong kind of thing where it is: a system holds
    /// boards and nothing else, and a board is placed only in a system. Reports it.
    fn misplaced(&mut self, def: &Definition, d: &Decl) -> bool {
        let board_member = def.kind == DefKind::Board;
        let message = if self.in_system() && !board_member {
            format!(
                "a system holds boards, but `{}` is a {}; put it on a board",
                def.name.name,
                def.kind.keyword()
            )
        } else if self.in_panel() && !board_member && def.kind != DefKind::Interface {
            format!(
                "a panel holds the board it repeats and its fabricator's panel rules, but `{}` is a {}",
                def.name.name,
                def.kind.keyword()
            )
        } else if !self.in_system()
            && !self.in_panel()
            && matches!(def.kind, DefKind::Board | DefKind::System)
        {
            format!(
                "a {} cannot be placed inside a {}; boards are placed only in a system or a panel",
                def.kind.keyword(),
                self.model.instance(self.current()).kind.word()
            )
        } else if self.in_panel() && def.kind == DefKind::Board && self.panel_has_cell() {
            "a panel repeats one board; this panel already names its board".to_string()
        } else if self.in_panel() && matches!(def.kind, DefKind::System | DefKind::Panel) {
            format!("a {} cannot be placed inside a panel", def.kind.keyword())
        } else {
            return false;
        };
        self.error(d.ty.segments[0].span, message);
        true
    }

    fn add_scalar(
        &mut self,
        owner: InstId,
        name: &str,
        kind: FieldKind,
        has_body: bool,
        span: Span,
    ) {
        if has_body {
            self.error(
                span,
                "a plain value has no members to set; remove the `{ ... }`",
            );
        }
        let field = self.model.add_field(owner, name, kind);
        self.model.add_member(owner, name, Member::Field(field));
    }

    pub(crate) fn add_pin(
        &mut self,
        parent: InstId,
        name: &str,
        span: Span,
        docs: Vec<String>,
    ) -> InstId {
        let pin = self.model.add_instance(Instance {
            name: name.to_string(),
            kind: InstKind::Pin,
            type_name: "Pin".to_string(),
            parent: Some(parent),
            children: Vec::new(),
            bridge: None,
            catalog: None,
            tables: Vec::new(),
            ties: Vec::new(),
            source: self.cur_source(),
            span,
            declared: None,
            docs,
            def: None,
        });
        for field in ["function", "net"] {
            let f = self.model.add_field(pin, field, FieldKind::Text);
            self.model.add_member(pin, field, Member::Field(f));
        }
        // What the pin survives, relative to its ground (ADR 0046): the solver checks the
        // worst-case voltage of its net against them.
        for field in ["abs_min", "abs_max"] {
            let f = self
                .model
                .add_field(pin, field, FieldKind::Quantity(Dimension::VOLTAGE));
            self.model.add_member(pin, field, Member::Field(f));
        }
        pin
    }

    /// Second pass: constraints, connections and the rest, now that every name exists.
    fn process(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Decl(d) => {
                    let key = (self.cur_source().0, d.span.start);
                    let Some(body) = &d.body else { continue };
                    if self.skipped.contains(&key) || d.name.segments.len() != 1 {
                        continue;
                    }
                    let cur = self.current();
                    if let Some(Member::Inst(child)) =
                        self.model.member(cur, &d.name.segments[0].name)
                    {
                        self.scope.push(child);
                        self.process(body);
                        self.scope.pop();
                    }
                }
                Item::Constraint(c) => self.constraint(c),
                Item::Connect(c) => self.connect(c),
                Item::Bridge(b) => self.bridge(b),
                Item::Catalog(c) => self.catalog(c),
                Item::Pins(p) => self.pins(p),
                Item::Tie(t) => self.tie(t),
            }
        }
    }

    // ---- names ---------------------------------------------------------------

    /// Whether `member` of `owner` may be named from the definition being written.
    ///
    /// A module is configured only through its interface: its rails, pins and values.
    /// The parts and modules inside it are its own business, so they can be named only
    /// from the module's own definition.
    fn visible(&mut self, owner: InstId, member: Member, name: &netsmith_lang::ast::Ident) -> bool {
        let inst = self.model.instance(owner);
        let outside =
            matches!(inst.kind, InstKind::Module | InstKind::Board) && inst.def != self.cur_def;
        let internal = match member {
            Member::Inst(child) => matches!(
                self.model.instance(child).kind,
                InstKind::Part | InstKind::Module | InstKind::Board
            ),
            _ => false,
        };
        if outside && internal && self.in_panel() && inst.kind == InstKind::Board {
            let ty = inst.type_name.clone();
            self.error(
                name.span,
                format!(
                    "`{}` is inside board `{ty}`; a panel repeats its board as it is and configures none of it",
                    name.name
                ),
            );
            return false;
        }
        // A connector that a board declares directly shows a system its `.mating` and nothing
        // else (ADR 0040).
        if outside && internal && self.in_system() && inst.kind == InstKind::Board {
            if let Member::Inst(child) = member
                && self.model.mating_of(child).is_some()
            {
                return true;
            }
            let ty = inst.type_name.clone();
            self.error(
                name.span,
                format!(
                    "`{}` is inside board `{ty}`; a system can reach only the ports a board declares directly, such as its rails, and the `.mating` of a connector",
                    name.name
                ),
            );
            return false;
        }
        if self.in_system()
            && inst.kind == InstKind::Part
            && name.name != "mating"
            && self
                .model
                .instance(owner)
                .parent
                .is_some_and(|b| self.model.instance(b).kind == InstKind::Board)
        {
            let (ty, part) = (inst.type_name.clone(), inst.name.clone());
            self.error(
                name.span,
                format!(
                    "`{}` is inside `{part}`, a `{ty}`; a system can reach only the `.mating` of a connector",
                    name.name
                ),
            );
            return false;
        }
        if outside && internal {
            let ty = inst.type_name.clone();
            self.error(
                name.span,
                format!(
                    "`{}` is inside `{ty}` and is not part of its interface; set it through a rail, a pin, or a value that `{ty}` declares",
                    name.name
                ),
            );
            return false;
        }
        true
    }

    pub fn resolve_path(&mut self, p: &Path) -> Res {
        let cur = self.current();
        let first = &p.segments[0];
        let start = if p.leading_dot {
            match self.model.member(cur, &first.name) {
                Some(m) => {
                    if !self.visible(cur, m, first) {
                        return Res::Bad;
                    }
                    m
                }
                None => {
                    let owner = self.model.instance(cur).type_name.clone();
                    self.error(
                        first.span,
                        format!("`{}` is not a member of this {owner}", first.name),
                    );
                    return Res::Bad;
                }
            }
        } else {
            let outer = self.scope.len() - 1;
            let found = self.scope[..outer]
                .iter()
                .rev()
                .find_map(|&s| self.model.member(s, &first.name).map(|m| (s, m)));
            match found {
                Some((owner, m)) => {
                    if !self.visible(owner, m, first) {
                        return Res::Bad;
                    }
                    m
                }
                None => {
                    let hint = if self.model.member(cur, &first.name).is_some() {
                        format!("; write `.{}` for a member of this block", first.name)
                    } else {
                        String::new()
                    };
                    self.error(
                        first.span,
                        format!(
                            "`{}` is not defined in any enclosing block{hint}",
                            first.name
                        ),
                    );
                    return Res::Bad;
                }
            }
        };

        let mut member = start;
        for seg in &p.segments[1..] {
            match member {
                Member::Poison => return Res::Poison,
                Member::Field(_) => {
                    self.error(seg.span, format!("a value has no member `{}`", seg.name));
                    return Res::Bad;
                }
                Member::Inst(i) => match self.model.member(i, &seg.name) {
                    Some(m) => {
                        if !self.visible(i, m, seg) {
                            return Res::Bad;
                        }
                        member = m;
                    }
                    None => {
                        let ty = self.model.instance(i).type_name.clone();
                        self.error(seg.span, format!("`{ty}` has no member `{}`", seg.name));
                        return Res::Bad;
                    }
                },
            }
        }
        match member {
            Member::Inst(i) => Res::Inst(i),
            Member::Field(f) => Res::Field(f),
            Member::Poison => Res::Poison,
        }
    }
}
