//! Loading modules, resolving imports and names, and elaborating definitions into a
//! [`Model`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use netsmith_lang::ast::{Decl, DefKind, Definition, Import, Item, Path, Physical, TypePath};
use netsmith_lang::{Span, parse};
use netsmith_quantity::Dimension;

use crate::diag::{Diagnostic, SourceId, Sources};
use crate::loader::Loader;
use crate::model::{
    EnumId, EnumInfo, FieldKind, GenericUse, InstId, InstKind, Instance, Member, Model,
};
use crate::providers::generic_is_part_shaped;

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
    /// A file of the standard library (its name starts with `std/`).
    pub std: bool,
    /// The module path of the file (`src::rf`, `std::parts::passive`): its name without
    /// `.nsm`, with `::` for `/`. What a provider's id starts with.
    pub key: String,
}

/// `src/rf.nsm` is the module `src::rf`.
fn module_key(file_name: &str) -> String {
    file_name
        .strip_suffix(".nsm")
        .unwrap_or(file_name)
        .replace('/', "::")
}

/// The module path of a file nothing loaded: its name without `.` and `..` steps, so that
/// `./src/main.nsm` and `src/main.nsm` are one module.
fn rootless_key(file_name: &str) -> String {
    module_key(&crate::loader::normalise_name(file_name))
}

/// A row of a generic's `pins` table, which a provider's row for the pin must agree with
/// (ADR 0068).
#[derive(Clone)]
pub(crate) struct ContractRow {
    /// The instance whose pin it is.
    pub inst: InstId,
    pub pin: InstId,
    pub kind: netsmith_lang::ast::PinKind,
    pub requirement: Option<netsmith_lang::ast::Requirement>,
    /// The generic that stated it.
    pub generic: String,
}

/// A `physical for` unit and the module (the physical file) that holds it.
type AttachedUnit = (usize, Rc<Physical>);

/// The physical file being read as part of a board (ADR 0048).
#[derive(Default)]
pub(crate) struct PhysState {
    /// Inside a `case { }` section.
    pub in_case: bool,
    /// The members of the board that the physical file declared (an outline, a keepout, a
    /// mounting hole): it may set any of their fields.
    pub declared: HashSet<InstId>,
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
    /// An `enum` (ADR 0054), by module and index in it.
    Enum(usize, usize),
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
    pub(crate) modules: Vec<ModuleData>,
    cache: HashMap<String, usize>,
    diags: Vec<Diagnostic>,
    /// The diagnostics of `diags` that are parse errors.
    syntax: Vec<Diagnostic>,
    seen: HashSet<Diagnostic>,
    // State of the elaboration in progress.
    pub model: Model,
    pub scope: Vec<InstId>,
    pub cur_module: usize,
    /// The definition whose source text is being checked. Anything else is "outside".
    pub(crate) cur_def: Option<(usize, usize)>,
    pub uf: HashMap<usize, usize>,
    skipped: HashSet<(usize, usize)>,
    /// Pins that a row of a `pins` table declared, as opposed to a `.name: Pin` member.
    pub(crate) table_pins: HashSet<InstId>,
    /// The `physical for` units of each board definition `(module, index)`, as the module
    /// that holds the unit and the unit. A board takes its units in when it is made, so
    /// that a `panel` or a `system` that places the board sees them too.
    attached: HashMap<(usize, usize), Vec<AttachedUnit>>,
    /// Set while a board's physical file is being read (ADR 0048).
    pub(crate) phys: Option<PhysState>,
    /// The fields that some `&=` statement states, in the elaboration in progress. A default
    /// (`?=`) gives way to one of these (ADR 0051).
    pub(crate) stated: HashSet<crate::model::FieldId>,
    /// The `?=` statements of the elaboration in progress, resolved when every statement has
    /// been read, so that their order never matters.
    pub(crate) defaults: Vec<crate::lower::PendingDefault>,
    /// The source a default is applied for, while it is applied after the statements are read.
    pub(crate) source_override: Option<SourceId>,
    /// The enums that fields of the model in progress have the type of, by the module and
    /// index of their definition.
    enum_ids: HashMap<(usize, usize), EnumId>,
    /// The providers chosen for the uses of the definition being elaborated: a use's path to
    /// a provider's id (ADR 0068).
    pub(crate) assign: BTreeMap<String, String>,
    /// The uses of generics found in the elaboration in progress.
    pub(crate) uses: Vec<GenericUse>,
    /// The members that a generic declared in an instance, by instance and name, with the
    /// generic's name: a provider may not declare one again (ADR 0068).
    pub(crate) generic_members: HashMap<(InstId, String), String>,
    /// The rows of the `pins` tables of generics, for the instance they were read into.
    pub(crate) contract_rows: Vec<ContractRow>,
    /// The definitions being made, outermost first: a definition that is made again inside
    /// itself would never end, so it is refused where it is declared.
    pub(crate) making: Vec<(usize, usize)>,
    /// Where the member being declared is written, for a message about what it makes.
    pub(crate) use_site: Option<(SourceId, Span)>,
    /// The loaded files as they were when the last model was made, shared by the models made
    /// while no file was added.
    snapshot: Option<std::sync::Arc<Sources>>,
}

/// The names that are in scope in every file without an `import` (ADR 0054): the enums that
/// the built-in members of every part have the type of. It is a file of the standard library.
const PRELUDE: &str = include_str!("../../../std/prelude.nsm");
/// Its module name, as `import` would write it.
pub const PRELUDE_MODULE: &str = "std::prelude";

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
            syntax: Vec::new(),
            seen: HashSet::new(),
            model: Model::new(),
            scope: Vec::new(),
            cur_module: 0,
            cur_def: None,
            uf: HashMap::new(),
            skipped: HashSet::new(),
            table_pins: HashSet::new(),
            attached: HashMap::new(),
            phys: None,
            stated: HashSet::new(),
            defaults: Vec::new(),
            source_override: None,
            enum_ids: HashMap::new(),
            assign: BTreeMap::new(),
            uses: Vec::new(),
            generic_members: HashMap::new(),
            contract_rows: Vec::new(),
            making: Vec::new(),
            use_site: None,
            snapshot: None,
        }
    }

    pub(crate) fn modules(&self) -> &[ModuleData] {
        &self.modules
    }

    pub fn into_diagnostics(self) -> (Vec<Diagnostic>, Vec<Diagnostic>, Sources) {
        (self.diags, self.syntax, self.sources)
    }

    /// Report a parse error: a diagnostic like any other, also remembered as one of syntax.
    fn report_syntax(&mut self, source: SourceId, e: netsmith_lang::ParseError) {
        let d = Diagnostic {
            source,
            span: e.span,
            message: e.message,
        };
        if !self.syntax.contains(&d) {
            self.syntax.push(d.clone());
        }
        self.report(d.source, d.span, d.message);
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

    pub(crate) fn in_kind(&self, kind: DefKind) -> bool {
        self.cur_def
            .is_some_and(|(m, i)| self.modules[m].defs[i].kind == kind)
    }

    /// Whether `source` is a file of the standard library, which ships with netsmith: its
    /// doc comments describe the library, not the design.
    pub(crate) fn is_library(&self, source: SourceId) -> bool {
        self.sources.get(source).name.starts_with("std/")
    }

    /// The file and the 1-based line of the start of `span` in `source`.
    pub(crate) fn line_in(&self, source: SourceId, span: Span) -> (String, usize) {
        let src = self.sources.get(source);
        let (line, _) = netsmith_lang::ParseError::new(span, "").line_col(&src.text);
        (src.name.clone(), line)
    }

    pub fn cur_source(&self) -> SourceId {
        self.source_override
            .unwrap_or(self.modules[self.cur_module].source)
    }

    pub fn error(&mut self, span: Span, message: impl Into<String>) {
        let source = self.cur_source();
        self.report(source, span, message);
    }

    // ---- modules and imports -------------------------------------------------

    /// The file the check is given. When the loader holds the same file as a module of the
    /// project (`src/main.nsm` is `src::main`, however the name was typed) the root is that
    /// module, cached under its key like every other file, so that a file which imports the
    /// root finds it and does not read a second copy (ADR 0068 section 11); a file the
    /// loader does not hold takes its module path from its name with the `.` steps removed.
    pub fn add_root(&mut self, name: &str, text: &str) -> usize {
        let named = self.loader.root_key(name);
        let key = named.clone().unwrap_or_else(|| rootless_key(name));
        // Only a module the loader holds can be imported, so only that is cached.
        let cached = named.filter(|k| self.loader.get(k).is_some());
        self.add_module_as(name, text, cached, key)
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
        self.add_module_as(name, text, key, module_key(name))
    }

    /// Read a file as a module with the module path `module_path`, cached under `key` (an
    /// import's path) if it has one.
    fn add_module_as(
        &mut self,
        name: &str,
        text: &str,
        key: Option<String>,
        module_path: String,
    ) -> usize {
        let source = self.sources.add(name, text);
        let mut file = match parse(text) {
            Ok(file) => file,
            Err(e) => {
                self.report_syntax(source, e);
                netsmith_lang::ast::File {
                    inner_docs: Vec::new(),
                    imports: Vec::new(),
                    definitions: Vec::new(),
                    physicals: Vec::new(),
                }
            }
        };
        let imports = std::mem::take(&mut file.imports);
        for unit in &file.physicals {
            self.report(
                source,
                unit.span,
                format!(
                    "`physical for {}` belongs in a file of its own, `{}`, beside this source; a source file holds the circuit",
                    unit.board.name,
                    physical_file_name(name)
                ),
            );
        }
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
            std: name.starts_with("std/"),
            key: module_path,
        });
        // Cached before the imports run so a cycle cannot recurse forever.
        if let Some(key) = key {
            self.cache.insert(key, id);
        }
        for import in &imports {
            self.apply_import(id, import);
        }
        let loader = self.loader;
        if let Some((physical_name, physical_text)) = loader.physical_of(name) {
            self.add_physical(id, physical_name, physical_text);
        }
        id
    }

    /// Read the physical file paired with module `owner` and attach each of its units to
    /// the board it names (ADR 0048).
    fn add_physical(&mut self, owner: usize, name: &str, text: &str) {
        let source = self.sources.add(name, text);
        let file = match parse(text) {
            Ok(file) => file,
            Err(e) => {
                self.report_syntax(source, e);
                return;
            }
        };
        for def in &file.definitions {
            self.report(
                source,
                def.name.span,
                format!(
                    "a physical file holds `physical for` units and nothing else; `{} {}` belongs in the circuit source",
                    def.kind.keyword(),
                    def.name.name
                ),
            );
        }
        let id = self.modules.len();
        self.modules.push(ModuleData {
            source,
            defs: Vec::new(),
            scope: HashMap::new(),
            std: false,
            key: module_key(name),
        });
        for import in &file.imports {
            self.apply_import(id, import);
        }
        let owner_defs = self.modules[owner].defs.clone();
        for unit in file.physicals {
            let unit = Rc::new(unit);
            let named = owner_defs
                .iter()
                .position(|d| d.name.name == unit.board.name);
            let Some(index) = named else {
                let boards: Vec<String> = owner_defs
                    .iter()
                    .filter(|d| d.kind == DefKind::Board)
                    .map(|d| format!("`{}`", d.name.name))
                    .collect();
                let known = if boards.is_empty() {
                    "it has no board".to_string()
                } else {
                    format!("its boards are {}", boards.join(", "))
                };
                self.report(
                    source,
                    unit.board.span,
                    format!(
                        "`physical for {}` names no board of the source it is paired with; {known}",
                        unit.board.name
                    ),
                );
                continue;
            };
            let kind = owner_defs[index].kind;
            if kind != DefKind::Board {
                self.report(
                    source,
                    unit.board.span,
                    format!(
                        "`physical for {}` names a {}, and a physical file for anything but a board is not built yet (ADR 0048); name a board, and reach the parts of a module by their path",
                        unit.board.name,
                        kind.keyword()
                    ),
                );
                continue;
            }
            let entry = self.attached.entry((owner, index)).or_default();
            if entry.iter().any(|(m, _)| *m == id) {
                self.report(
                    source,
                    unit.board.span,
                    format!(
                        "`physical for {}` is already given in this file; keep one unit for each board",
                        unit.board.name
                    ),
                );
                continue;
            }
            entry.push((id, unit));
        }
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
            Some(Binding::Def(m, i)) => self.def_type(m, i),
            Some(Binding::Poison) => Ty::Poison,
            None => match self.prelude_def(&first.name) {
                Some((m, i)) => self.def_type(m, i),
                None => match builtin(&first.name) {
                    Some(b) => Ty::Builtin(b),
                    None => {
                        self.error(first.span, format!("unknown type `{}`", first.name));
                        Ty::Bad
                    }
                },
            },
        }
    }

    fn def_type(&self, module: usize, index: usize) -> Ty {
        if self.modules[module].defs[index].kind == DefKind::Enum {
            Ty::Enum(module, index)
        } else {
            Ty::Def(module, index)
        }
    }

    /// The prelude, read the first time a name is looked up in it (ADR 0054).
    fn prelude(&mut self) -> usize {
        match self.cache.get(PRELUDE_MODULE) {
            Some(&id) => id,
            None => self.add_module("std/prelude.nsm", PRELUDE, Some(PRELUDE_MODULE.to_string())),
        }
    }

    /// A definition of the prelude by name.
    fn prelude_def(&mut self, name: &str) -> Option<(usize, usize)> {
        let module = self.prelude();
        match self.modules[module].scope.get(name) {
            Some(Binding::Def(m, i)) => Some((*m, *i)),
            _ => None,
        }
    }

    /// The id that fields of the model in progress use for an enum.
    pub(crate) fn enum_id(&mut self, module: usize, index: usize) -> EnumId {
        if let Some(&id) = self.enum_ids.get(&(module, index)) {
            return id;
        }
        let def = &self.modules[module].defs[index];
        let info = EnumInfo {
            name: def.name.name.clone(),
            variants: def.variants.iter().map(|v| v.name.name.clone()).collect(),
        };
        self.model.enums.push(info);
        let id = EnumId(self.model.enums.len() - 1);
        self.enum_ids.insert((module, index), id);
        id
    }

    /// The enum of the prelude that the built-in members of a part have the type of.
    pub(crate) fn assembly_mode(&mut self) -> FieldKind {
        let (m, i) = self
            .prelude_def("AssemblyMode")
            .expect("the prelude defines AssemblyMode");
        FieldKind::Enum(self.enum_id(m, i))
    }

    // ---- elaboration ---------------------------------------------------------

    /// The loaded files, shared with the earlier models when nothing was loaded since.
    fn source_snapshot(&mut self) -> std::sync::Arc<Sources> {
        match &self.snapshot {
            Some(s) if s.len() == self.sources.len() => s.clone(),
            _ => {
                let s = std::sync::Arc::new(self.sources.clone());
                self.snapshot = Some(s.clone());
                s
            }
        }
    }

    /// Build a fresh model of one definition, with the checks that need the whole tree.
    pub fn elaborate(
        &mut self,
        module: usize,
        index: usize,
        assign: Option<&BTreeMap<String, String>>,
    ) -> Model {
        self.model = Model::new();
        self.assign = assign.cloned().unwrap_or_default();
        self.uses.clear();
        self.generic_members.clear();
        self.contract_rows.clear();
        self.making.clear();
        self.use_site = None;
        self.scope.clear();
        self.uf.clear();
        self.skipped.clear();
        self.table_pins.clear();
        self.stated.clear();
        self.defaults.clear();
        self.enum_ids.clear();
        self.source_override = None;
        let def = self.modules[module].defs[index].clone();
        let name = def.name.name.clone();
        let root = self
            .instantiate(&def, (module, index), &name, None)
            .expect("the root is made inside nothing, so nothing repeats");
        self.model.root = root;
        self.resolve_defaults();
        self.finalize_nets();
        self.model.uses = std::mem::take(&mut self.uses);
        self.model.sources = crate::model::SourceTexts(Some(self.source_snapshot()));
        std::mem::replace(&mut self.model, Model::new())
    }

    fn instantiate(
        &mut self,
        def: &Rc<Definition>,
        def_ref: (usize, usize),
        name: &str,
        parent: Option<InstId>,
    ) -> Option<InstId> {
        match parent {
            // A member declared with the type of a generic stands for one of its providers
            // (ADR 0068). A generic that is elaborated on its own is checked as it is.
            Some(parent) if def.kind == DefKind::Generic => {
                self.instantiate_use(def, def_ref, name, parent)
            }
            // A definition inside itself, or inside what it holds, never ends: refused.
            Some(_) if self.making.contains(&def_ref) => {
                self.report_cycle(name, None, def_ref);
                None
            }
            _ => Some(self.instantiate_as(def, def_ref, name, parent, None)),
        }
    }

    pub(crate) fn instantiate_as(
        &mut self,
        def: &Rc<Definition>,
        def_ref: (usize, usize),
        name: &str,
        parent: Option<InstId>,
        placed: Option<((usize, usize), String)>,
    ) -> InstId {
        // One for each instance made: a board of a thousand parts is a thousand of these.
        netsmith_core::job::checkpoint();
        let kind = match def.kind {
            DefKind::Board => InstKind::Board,
            DefKind::System => InstKind::System,
            DefKind::Panel => InstKind::Panel,
            DefKind::Module => InstKind::Module,
            DefKind::Part => InstKind::Part,
            DefKind::Interface => InstKind::Interface,
            // A generic is made as a part when it has pins or a catalog, else as a module.
            DefKind::Generic if generic_is_part_shaped(def) => InstKind::Part,
            DefKind::Generic => InstKind::Module,
            DefKind::Enum => unreachable!("an enum is a type, so nothing instantiates it"),
        };
        self.making.push(def_ref);
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
            rules: Default::default(),
            source: self.modules[def_ref.0].source,
            span: def.span,
            declared: None,
            docs: def.docs.clone(),
            def_docs: if self.is_library(self.modules[def_ref.0].source) {
                Vec::new()
            } else {
                def.docs.iter().chain(&def.inner_docs).cloned().collect()
            },
            doc_at: (!self.is_library(self.modules[def_ref.0].source)
                && (!def.docs.is_empty() || !def.inner_docs.is_empty()))
            .then(|| self.line_in(self.modules[def_ref.0].source, def.span)),
            def: Some(def_ref),
            provides: def.provides.iter().map(|p| p.name.clone()).collect(),
            provides_std: Vec::new(),
            placed_as: placed.as_ref().map(|(_, n)| n.clone()),
            placed_def: placed.map(|(d, _)| d),
            std: self.modules[def_ref.0].std,
        });
        // Every part has a field for who assembles it (ADR 0024): the board narrows it, the
        // catalog row may state a default, and lowering reads it. It has one for the letters
        // of its reference designator too (ADR 0046): its kind states them, a user part or a
        // board may narrow it, and lowering reads it.
        if kind == InstKind::Part {
            let assembly = self.assembly_mode();
            for (name, kind) in [("assembly", assembly), ("designator", FieldKind::Text)] {
                let field = self.model.add_field(id, name, kind);
                self.model.add_member(id, name, Member::Field(field));
            }
            // And one for how tall its body stands above the board (ADR 0045, "Height
            // limits", PROVISIONAL): the part or the board states it, and lowering reads it.
            // A part that does not has no height, which is not a height of zero.
            let field = self
                .model
                .add_field(id, "height", FieldKind::Quantity(Dimension::LENGTH));
            self.model.add_member(id, "height", Member::Field(field));
            // And one for how far its pins stand out of the board, on the face opposite the
            // part, once they are cut short (ADR 0045 section 15, PROVISIONAL): the board's
            // physical file states it for an instance, the kind may bound it. A part that
            // does not has no known protrusion, which is not a protrusion of zero.
            let field =
                self.model
                    .add_field(id, "pin_protrusion", FieldKind::Quantity(Dimension::LENGTH));
            self.model
                .add_member(id, "pin_protrusion", Member::Field(field));
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
        // A definition's own text is circuit source, even when a physical file is what
        // asked for it to be made.
        let saved_phys = self.phys.take();
        // A part or module that provides generics has their members and statements first
        // (ADR 0068), each read in its own file.
        let generics = self.provided(def);
        self.model.instances[id.0].provides_std = generics
            .iter()
            .filter(|(_, gref)| self.modules[gref.0].std)
            .map(|(g, _)| g.name.name.clone())
            .collect();
        for (g, gref) in &generics {
            let items = self.generic_items(g, def, kind);
            let before = self.model.instance(id).children.len();
            let saved = self.enter_def(*gref);
            self.declare(&items);
            self.leave_def(saved);
            let names: Vec<String> = self.model.instance(id).children[before..]
                .iter()
                .map(|(n, _)| n.clone())
                .collect();
            for n in names {
                self.generic_members
                    .entry((id, n))
                    .or_insert_with(|| g.name.name.clone());
            }
        }
        self.declare(&def.items);
        for (g, gref) in &generics {
            let items = self.generic_items(g, def, kind);
            let saved = self.enter_def(*gref);
            self.process(&items);
            self.leave_def(saved);
        }
        self.process(&def.items);
        if kind == InstKind::Part {
            self.finish_rules(id);
        }
        if !generics.is_empty() {
            self.conform(id, def, &generics);
        }
        if kind == InstKind::Board {
            self.merge_physical(def_ref);
        }
        self.phys = saved_phys;
        self.cur_module = saved_module;
        self.cur_def = saved_def;
        self.scope = saved_scope;
        self.making.pop();
        id
    }

    /// Make the reading of a definition's text happen in its own file; the pair undoes it.
    pub(crate) fn enter_def(&mut self, def_ref: (usize, usize)) -> (usize, Option<(usize, usize)>) {
        let module = std::mem::replace(&mut self.cur_module, def_ref.0);
        (module, self.cur_def.replace(def_ref))
    }

    pub(crate) fn leave_def(&mut self, saved: (usize, Option<(usize, usize)>)) {
        self.cur_module = saved.0;
        self.cur_def = saved.1;
    }

    /// Read the physical file of the board being made, as part of the board (ADR 0048): its
    /// declarations become members of the board, and its statements are checked with the
    /// reach into modules that a physical file has and a circuit source does not.
    fn merge_physical(&mut self, board: (usize, usize)) {
        let Some(units) = self.attached.get(&board).cloned() else {
            return;
        };
        let saved_module = self.cur_module;
        for (module, unit) in units {
            self.cur_module = module;
            let saved = self.phys.replace(PhysState::default());
            self.declare(&unit.items);
            self.process(&unit.items);
            self.phys = saved;
        }
        self.cur_module = saved_module;
    }

    /// The built-in `.place` of a part or module (ADR 0045): an interface `Place` with the
    /// box of the body in the board's frame (`x` and `y` of its centre, `left`, `right`,
    /// `top` and `bottom` of its edges), the turn in degrees and the face. A module's has
    /// no `rotation`: the placer does not turn a module as a block. Its `side` is a hard
    /// requirement on every part of it (ADR 0048, "Syntax decisions" 5).
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
            rules: Default::default(),
            source,
            span,
            declared: None,
            docs: Vec::new(),
            def_docs: Vec::new(),
            doc_at: None,
            def: None,
            provides: Vec::new(),
            provides_std: Vec::new(),
            placed_as: None,
            placed_def: None,
            std: false,
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
            ("side", FieldKind::Text, false),
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
                if self.phys.is_none() {
                    self.declare_table_pins(table);
                }
                continue;
            }
            if let Item::Case(case) = item {
                if let Some(phys) = &mut self.phys {
                    let outer = std::mem::replace(&mut phys.in_case, true);
                    self.declare(&case.items);
                    if let Some(phys) = &mut self.phys {
                        phys.in_case = outer;
                    }
                } else {
                    self.error(case.span, CASE_OUTSIDE);
                }
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
                let message = self.redeclared(cur, &name, d);
                self.error(d.name.span, message);
                self.skipped.insert(key);
                continue;
            }

            let ty = self.resolve_type(&d.ty);
            if self.in_kind(DefKind::Generic)
                && let Ty::Def(m, i) = &ty
                && self.modules[*m].defs[*i].kind != DefKind::Interface
            {
                let (ty_name, word) = {
                    let def = &self.modules[*m].defs[*i];
                    (def.name.name.clone(), crate::providers::kind_word(def))
                };
                self.error(
                    d.ty.segments[0].span,
                    format!(
                        "a generic holds values, pins and interfaces, and no parts or modules; `{name}` is a {word} (`{ty_name}`): put it in a provider"
                    ),
                );
                self.model.add_member(cur, &name, Member::Poison);
                continue;
            }
            if !self.fits_here(&ty, d) {
                self.model.add_member(cur, &name, Member::Poison);
                continue;
            }
            if (self.in_system() || self.in_panel()) && matches!(ty, Ty::Builtin(_) | Ty::Enum(..))
            {
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
                Ty::Builtin(Builtin::Quantity(dim)) => {
                    self.add_scalar(
                        cur,
                        &name,
                        FieldKind::Quantity(dim),
                        d.body.is_some(),
                        d.span,
                    );
                }
                Ty::Builtin(Builtin::Text) => {
                    self.add_scalar(cur, &name, FieldKind::Text, d.body.is_some(), d.span);
                }
                Ty::Enum(m, i) => {
                    let kind = FieldKind::Enum(self.enum_id(m, i));
                    self.add_scalar(cur, &name, kind, d.body.is_some(), d.span);
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
                    let last = d.ty.segments.last().map_or(d.name.span.end, |s| s.span.end);
                    let site = (self.cur_source(), Span::new(d.name.span.start, last));
                    self.use_site = Some(site);
                    let Some(child) = self.instantiate(&def, (m, i), &name, Some(cur)) else {
                        self.model.add_member(cur, &name, Member::Poison);
                        continue;
                    };
                    self.model.instances[child.0].docs = d.docs.clone();
                    if !d.docs.is_empty() && !self.is_library(self.cur_source()) {
                        let at = self.line_in(self.cur_source(), d.span);
                        self.model.instances[child.0].doc_at = Some(at);
                    }
                    self.model.instances[child.0].declared = Some(site);
                    self.model.add_member(cur, &name, Member::Inst(child));
                    self.no_provider(child, d);
                    if self.scope.len() == 1
                        && let Some(phys) = &mut self.phys
                    {
                        phys.declared.insert(child);
                    }
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
                crate::providers::kind_word(def)
            )
        } else if self.in_panel() && !board_member && def.kind != DefKind::Interface {
            format!(
                "a panel holds the board it repeats and its fabricator's panel rules, but `{}` is a {}",
                def.name.name,
                crate::providers::kind_word(def)
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
            rules: Default::default(),
            source: self.cur_source(),
            span,
            declared: None,
            docs,
            def_docs: Vec::new(),
            doc_at: None,
            def: None,
            provides: Vec::new(),
            provides_std: Vec::new(),
            placed_as: None,
            placed_def: None,
            std: false,
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
                Item::Case(case) => {
                    if let Some(phys) = &mut self.phys {
                        let outer = std::mem::replace(&mut phys.in_case, true);
                        self.process(&case.items);
                        if let Some(phys) = &mut self.phys {
                            phys.in_case = outer;
                        }
                    }
                }
                Item::Row(r) => self.row(r),
                Item::Connect(c) if self.phys.is_none() => self.connect(c),
                Item::Bridge(b) if self.phys.is_none() => self.bridge(b),
                Item::Catalog(c) if self.phys.is_none() => self.catalog(c),
                Item::Pins(p) if self.phys.is_none() => self.pins(p),
                Item::Tie(t) if self.phys.is_none() => self.tie(t),
                Item::Peripheral(p) if self.phys.is_none() => self.peripheral(p),
                Item::Shared(s) if self.phys.is_none() => self.shared(s),
                Item::Connect(c) => self.not_physical(c.span, "a connection with `~`"),
                Item::Bridge(b) => self.not_physical(b.span, "a `bridge`"),
                Item::Catalog(c) => self.not_physical(c.span, "a `catalog`"),
                Item::Pins(p) => self.not_physical(p.span, "a `pins` table"),
                Item::Tie(t) => self.not_physical(t.span, "a `tie`"),
                Item::Peripheral(p) => self.not_physical(p.span, "a `peripheral`"),
                Item::Shared(s) => self.not_physical(s.span, "a `shared` resource"),
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
        // A board's physical file names a part inside a module by its path (ADR 0048). It
        // may only state physical intent about it: `physical_gate` refuses the rest.
        if self.phys.is_some() {
            return true;
        }
        // A use of a generic reaches the members the generic declares, so that the board works
        // with whichever provider is picked (ADR 0068).
        if let Some(generic) = self.outside_contract(owner, &name.name) {
            let ty = self.model.instance(owner).type_name.clone();
            self.error(
                name.span,
                format!(
                    "`{}` is not declared by `{generic}`; a use of a generic reaches only the members the generic declares, so that it holds for every provider (declare it in `{generic}`, or name a provider, `{ty}`, as the type)",
                    name.name
                ),
            );
            return false;
        }
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

const CASE_OUTSIDE: &str = "a `case { }` section belongs in a physical file (`physical for Board { case { ... } }`, ADR 0048)";

/// `led.nsm` has its physical file in `led.physical.nsm` (ADR 0048).
pub fn physical_file_name(source: &str) -> String {
    format!(
        "{}.physical.nsm",
        source.strip_suffix(".nsm").unwrap_or(source)
    )
}

/// The things a board's physical file declares (ADR 0048, "Syntax decisions" 7 and 8): the
/// outline, cutouts, mounting holes, fiducials and labels belong in its `case` section;
/// the keepouts a board draws may stand anywhere in the file.
struct PhysicalType {
    /// With the article: `an outline`.
    noun: &'static str,
    in_case: bool,
}

impl Checker<'_> {
    fn physical_type(&self, ty: &Ty) -> Option<PhysicalType> {
        let Ty::Def(m, i) = ty else { return None };
        if !self.modules[*m].std {
            return None;
        }
        let def = &self.modules[*m].defs[*i];
        let (noun, in_case) = match (def.kind, def.name.name.as_str()) {
            (DefKind::Interface, "Outline") => ("an outline", true),
            (DefKind::Interface, "Cutout") => ("a cutout", true),
            (DefKind::Interface, "Keepout") => ("a keepout that a board draws", false),
            (DefKind::Part, "MountingHole") => ("a mounting hole", true),
            (DefKind::Part, "Fiducial") => ("a fiducial", true),
            (DefKind::Part, "Label") => ("a label", true),
            _ => return None,
        };
        Some(PhysicalType { noun, in_case })
    }

    /// The name of the board whose text is being read, and of the physical file it would
    /// have, for a message.
    fn physical_home(&self) -> (String, String) {
        let board = self
            .cur_def
            .map(|(m, i)| self.modules[m].defs[i].name.name.clone())
            .unwrap_or_default();
        let source = &self.sources.get(self.cur_source()).name;
        (board, physical_file_name(source))
    }

    /// Whether a declaration may stand where it is. A board's circuit source does not hold
    /// the outline, cutouts, mounting holes, fiducials, labels or keepouts of the board
    /// (ADR 0048 decision 10: the old form is an error), and a physical file holds nothing
    /// but those. Reports it when not.
    fn fits_here(&mut self, ty: &Ty, d: &Decl) -> bool {
        let physical = self.physical_type(ty);
        let (board, file) = self.physical_home();
        let member = d.name.to_source();
        let type_name = d.ty.segments.last().map_or("", |s| s.name.as_str());
        if let Some(phys) = &self.phys {
            if self.scope.len() != 1 {
                return true;
            }
            let in_case = phys.in_case;
            return match physical {
                None => {
                    self.error(
                        d.ty.segments[0].span,
                        format!(
                            "a physical file declares the outline, cutouts, keepouts, mounting holes, fiducials and labels of a board; `{member}: {type_name}` is none of them, and a part or a module belongs in the circuit source"
                        ),
                    );
                    false
                }
                Some(kind) if kind.in_case && !in_case => {
                    self.error(
                        d.ty.segments[0].span,
                        format!(
                            "{} belongs in the `case {{ }}` section of `physical for {board}`: wrap `{member}: {type_name}` in `case {{ ... }}`",
                            kind.noun
                        ),
                    );
                    false
                }
                Some(_) => true,
            };
        }
        if !self.in_board() {
            return true;
        }
        let Some(kind) = physical else { return true };
        let place = if kind.in_case {
            format!("the `case {{ }}` section of `physical for {board}` in `{file}`")
        } else {
            format!(
                "`physical for {board}` in `{file}` (a keepout that a part carries stays in the part)"
            )
        };
        self.error(
            d.ty.segments[0].span,
            format!(
                "`{member}: {type_name}` is {}, which is physical intent, and a board's circuit source does not hold it (ADR 0048); declare it in {place}",
                kind.noun
            ),
        );
        false
    }

    /// Report a statement that a physical file may not hold.
    fn not_physical(&mut self, span: Span, what: &str) {
        self.error(
            span,
            format!(
                "a physical file states where things are, and {what} changes the circuit; put it in the circuit source"
            ),
        );
    }

    /// Whether a statement of a board may mention these fields. A physical file may set and
    /// compare only physical things: a part's `.place`, a part's `.height` and `.pin_protrusion`, and what the
    /// file declared itself. A board's circuit source may mention none of a `.place`
    /// (ADR 0048 decision 10). Reports it when not.
    pub(crate) fn physical_gate(&mut self, fields: &[crate::model::FieldId], span: Span) -> bool {
        if let Some(phys) = &self.phys {
            for &f in fields {
                if !self.physical_field(f, phys) {
                    let path = self.model.path_of_field(f);
                    self.error(
                        span,
                        format!(
                            "`{path}` is not physical intent: a physical file states where a part is (its `.place`), how tall it is (`.height`), how far its pins protrude (`.pin_protrusion`) and the outline, cutouts, keepouts, holes, fiducials and labels it declares, never what a part is; put this in the circuit source"
                        ),
                    );
                    return false;
                }
            }
            return true;
        }
        if !self.in_board() {
            return true;
        }
        if let Some(&f) = fields.iter().find(|&&f| self.model.is_place_field(f)) {
            let path = self.model.path_of_field(f);
            let (board, file) = self.physical_home();
            self.error(
                span,
                format!(
                    "`{path}` is a placement requirement, which is physical intent, and a board's circuit source does not state it (ADR 0048); put it in `physical for {board}` in `{file}`"
                ),
            );
            return false;
        }
        true
    }

    fn physical_field(&self, field: crate::model::FieldId, phys: &PhysState) -> bool {
        if self.model.is_place_field(field) {
            return true;
        }
        let f = self.model.field(field);
        if (f.name == "height" || f.name == "pin_protrusion")
            && self.model.instance(f.owner).kind == InstKind::Part
        {
            return true;
        }
        let mut cur = Some(f.owner);
        while let Some(i) = cur {
            if phys.declared.contains(&i) {
                return true;
            }
            cur = self.model.instance(i).parent;
        }
        false
    }
}
