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
    /// A variant of an enum, imported by name (`import std::parts::passive::Color::Red`,
    /// ADR 0118): the enum's definition and the variant's index.
    Variant(usize, usize, usize),
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
    /// A type parameter of the module being read (`Ic`, ADR 0115): the part that stands for
    /// it and the generic that bounds it.
    Param {
        arg: (usize, usize),
        bound: (usize, usize),
        name: String,
    },
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
    pub(crate) diags: Vec<Diagnostic>,
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
    /// The interfaces that `unused` statements of the board name (ADR 0083), until they act.
    pub(crate) unused_stmts: Vec<crate::model::Unused>,
    /// The operands of the `~` and `~>` statements read so far, where a message about a
    /// connection points.
    pub(crate) touches: Vec<crate::unused::Touch>,
    /// The `?=` statements of the elaboration in progress, resolved when every statement has
    /// been read, so that their order never matters.
    pub(crate) defaults: Vec<crate::lower::PendingDefault>,
    /// The source a default is applied for, while it is applied after the statements are read.
    pub(crate) source_override: Option<SourceId>,
    /// The enums that fields of the model in progress have the type of, by the module and
    /// index of their definition.
    pub(crate) enum_ids: HashMap<(usize, usize), EnumId>,
    /// The interface definitions whose `lines` had a mistake (ADR 0064), so that the rules do not
    /// read them and the mistake is said once.
    pub(crate) bus_def_failed: HashSet<(usize, usize)>,
    /// The providers chosen for the uses of the definition being elaborated: a use's path to
    /// a provider's id (ADR 0068).
    pub(crate) assign: BTreeMap<String, String>,
    /// The uses of generics found in the elaboration in progress.
    pub(crate) uses: Vec<GenericUse>,
    /// The contradictions found inside a use with several providers (ADR 0113).
    pub(crate) provider_conflicts: Vec<Diagnostic>,
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
    /// Whether `min(.f)` and `max(.f)` may be read: set while the two sides of a `<=` or `>=`
    /// statement are lowered, so that nothing is ever solved from the end of a range.
    pub(crate) range_ends: bool,
    /// The loaded files as they were when the last model was made, shared by the models made
    /// while no file was added.
    snapshot: Option<std::sync::Arc<Sources>>,
    /// The pin file beside the top-level source, read (ADR 0101).
    pub(crate) pin_input: Option<PinInput>,
    /// What each board's or system's elaboration decided about pins.
    pub(crate) pin_roots: Vec<crate::assign::RootPins>,
    /// What the pin file says that is stale but does not stop the run: an entry that no longer
    /// holds and is picked again, an entry of a request that is gone.
    pub(crate) pin_findings: Vec<String>,
    /// The type parameters of the module whose text is being read, with what stands for each
    /// (ADR 0115).
    pub(crate) params: Option<crate::params::ParamEnv>,
    /// The arguments of the next parametric module made, set just before it is made.
    pub(crate) next_args: Option<Vec<(usize, usize)>>,
    /// The members made from a type parameter, with the parameter's name: a path through one
    /// reaches only the members of the parameter's bound.
    pub(crate) param_insts: HashMap<InstId, String>,
    /// The modules made with type arguments, each parameter of theirs with the parameter of
    /// the enclosing module it was given (`.buck: Buck<Ic>` gives `Ic` for `Ic`), or `None`
    /// where a part was named (`Buck<Lt8641>`). A join follows it to the IC (ADR 0120).
    pub(crate) param_args: HashMap<InstId, Vec<(String, Option<String>)>>,
    /// Set while the left side of a `&=` is resolved: the one place a path may reach the
    /// parts and modules inside a module (ADR 0116).
    pub(crate) narrowing: bool,
    /// Set while an end of a `~` or `~>` is resolved: a join reaches the pins of the IC that a
    /// module's user named for its type parameter (ADR 0120).
    pub(crate) joining: bool,
    /// Set while the `support` section of a part is read (ADR 0115).
    pub(crate) in_support: bool,
}

/// The pin file of the top-level source (ADR 0101): where it is among the loaded files, and
/// what it says.
pub(crate) struct PinInput {
    pub source: SourceId,
    pub file: crate::pinfile::PinFile,
}

/// The names that are in scope in every file without an `import` (ADR 0054): the enums that
/// the built-in members of every part have the type of. It is a file of the standard library.
const PRELUDE: &str = include_str!("../../../std/prelude.nsm");
/// Its module name, as `import` would write it.
pub const PRELUDE_MODULE: &str = "std::prelude";

/// The members of every part that name the catalog row it is (ADR 0122): narrowing either is a
/// hand pick.
pub(crate) const HAND_PICK: [&str; 2] = ["lcsc", "mpn"];

fn builtin(name: &str) -> Option<Builtin> {
    use Dimension as D;
    let q = |d| Some(Builtin::Quantity(d));
    match name {
        "Pin" => Some(Builtin::Pin),
        "String" | "Package" => Some(Builtin::Text),
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
            narrowing: false,
            joining: false,
            stated: HashSet::new(),
            unused_stmts: Vec::new(),
            touches: Vec::new(),
            defaults: Vec::new(),
            source_override: None,
            enum_ids: HashMap::new(),
            bus_def_failed: HashSet::new(),
            assign: BTreeMap::new(),
            uses: Vec::new(),
            provider_conflicts: Vec::new(),
            generic_members: HashMap::new(),
            contract_rows: Vec::new(),
            making: Vec::new(),
            use_site: None,
            range_ends: false,
            snapshot: None,
            pin_input: None,
            pin_roots: Vec::new(),
            pin_findings: Vec::new(),
            params: None,
            next_args: None,
            param_insts: HashMap::new(),
            param_args: HashMap::new(),
            in_support: false,
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
        let id = self.add_module_as(name, text, cached, key);
        self.load_pin_file(name);
        id
    }

    /// Read the pin file paired with the top-level file `name`, if the loader has one. A file
    /// that does not read is reported, and the run goes on as if there were none.
    fn load_pin_file(&mut self, name: &str) {
        let loader = self.loader;
        let Some((file_name, text)) = loader.pins_of(name) else {
            return;
        };
        let source = self.sources.add(file_name, text);
        let file = match crate::pinfile::PinFile::parse(text) {
            Ok(file) => file,
            Err(errors) => {
                for e in errors {
                    self.report(source, e.span, e.message);
                }
                crate::pinfile::PinFile::default()
            }
        };
        self.pin_input = Some(PinInput { source, file });
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
            Ok(mut file) => {
                for e in netsmith_lang::expand_groups(&mut file) {
                    self.report_syntax(source, e);
                }
                file
            }
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
            Ok(mut file) => {
                for e in netsmith_lang::expand_groups(&mut file) {
                    self.report_syntax(source, e);
                }
                file
            }
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

        if module_path.is_empty() {
            self.report(
                source,
                import.span,
                "an import needs a module path before the name",
            );
            self.bind_imported(module, source, &names, |_, _| Binding::Poison);
            return;
        }
        let target = self.load_module(&module_path);
        // `import a::b::Enum::Variant` or `import a::b::Enum::{X, Y}`: the variants of an enum
        // of module `a::b` (ADR 0118).
        if target.is_none()
            && let Some((enum_name, parent)) = module_path.split_last()
            && !parent.is_empty()
            && let Some(m) = self.load_module(parent)
            && let Some(i) = self.modules[m]
                .defs
                .iter()
                .position(|d| d.name.name == *enum_name && d.kind == DefKind::Enum)
        {
            let variants: Vec<String> = self.modules[m].defs[i]
                .variants
                .iter()
                .map(|v| v.name.name.clone())
                .collect();
            let enum_name = enum_name.clone();
            let mut missing = Vec::new();
            for (name, span) in &names {
                if !variants.contains(name) {
                    missing.push((name.clone(), *span));
                }
            }
            for (name, span) in missing {
                let listed: Vec<String> = variants.iter().map(|v| format!("`{v}`")).collect();
                let hint = crate::lower::closest(&name, &variants)
                    .map(|v| format!("; did you mean `{v}`?"))
                    .unwrap_or_default();
                self.report(
                    source,
                    span,
                    format!(
                        "`{enum_name}` has no variant `{name}`; its variants are {}{hint}",
                        crate::lower::list_of(&listed)
                    ),
                );
            }
            self.bind_imported(module, source, &names, |name, _| {
                match variants.iter().position(|v| v == name) {
                    Some(v) => Binding::Variant(m, i, v),
                    None => Binding::Poison,
                }
            });
            return;
        }
        if target.is_none() {
            self.report(
                source,
                import.span,
                format!("unknown module `{}`", module_path.join("::")),
            );
        }
        let mut bindings = Vec::new();
        for (name, span) in &names {
            let binding = match target {
                None => Binding::Poison,
                Some(t) => match self.modules[t]
                    .defs
                    .iter()
                    .position(|d| d.name.name == *name)
                {
                    Some(index) => Binding::Def(t, index),
                    None => {
                        self.report(
                            source,
                            *span,
                            format!(
                                "module `{}` has no definition `{name}`",
                                module_path.join("::")
                            ),
                        );
                        Binding::Poison
                    }
                },
            };
            bindings.push(binding);
        }
        self.bind_imported(module, source, &names, |_, at| bindings[at]);
    }

    /// Put the imported names in the scope of `module`, each with the binding `of` gives it
    /// (by name and by its place in the list).
    fn bind_imported(
        &mut self,
        module: usize,
        source: SourceId,
        names: &[(String, Span)],
        of: impl Fn(&str, usize) -> Binding,
    ) {
        for (at, (name, span)) in names.iter().enumerate() {
            let binding = of(name, at);
            if self.modules[module]
                .scope
                .insert(name.clone(), binding)
                .is_some()
            {
                self.report(
                    source,
                    *span,
                    format!("`{name}` is already defined or imported"),
                );
            }
        }
    }

    /// The variant a bare name stands for in the current file, if the file imports it.
    pub(crate) fn imported_variant(&self, name: &str) -> Option<(usize, usize, String)> {
        match self.modules[self.cur_module].scope.get(name) {
            Some(Binding::Variant(m, i, v)) => Some((
                *m,
                *i,
                self.modules[*m].defs[*i].variants[*v].name.name.clone(),
            )),
            _ => None,
        }
    }

    /// How a bare name could be written as a variant of an enum the file can see (its own,
    /// an imported one or the prelude's): `` `AssemblyMode::Hand` `` and the import of the
    /// variant, for the message of a name that is not in scope.
    pub(crate) fn variant_hint(&mut self, name: &str) -> Option<String> {
        let mut enums: Vec<(usize, usize)> = self.modules[self.cur_module]
            .scope
            .values()
            .filter_map(|b| match b {
                Binding::Def(m, i) if self.modules[*m].defs[*i].kind == DefKind::Enum => {
                    Some((*m, *i))
                }
                _ => None,
            })
            .collect();
        let prelude = self.prelude();
        enums.extend(
            self.modules[prelude]
                .defs
                .iter()
                .enumerate()
                .filter(|(_, d)| d.kind == DefKind::Enum)
                .map(|(i, _)| (prelude, i)),
        );
        enums.sort();
        enums.dedup();
        enums.into_iter().find_map(|(m, i)| {
            let def = &self.modules[m].defs[i];
            let variants: Vec<String> = def.variants.iter().map(|v| v.name.name.clone()).collect();
            let variant = crate::lower::closest(name, &variants)?;
            Some(self.variant_spellings(m, i, &variant))
        })
    }

    /// `` `Color::Red`, or import the variant: `import std::parts::passive::Color::Red` ``.
    pub(crate) fn variant_spellings(&self, m: usize, i: usize, variant: &str) -> String {
        let name = &self.modules[m].defs[i].name.name;
        format!(
            "`{name}::{variant}`, or import the variant: `import {}::{name}::{variant}`",
            self.modules[m].key
        )
    }

    pub fn resolve_type(&mut self, tp: &TypePath) -> Ty {
        let first = &tp.segments[0];
        let bound = self.modules[self.cur_module]
            .scope
            .get(&first.name)
            .copied();
        if tp.segments.len() == 1
            && !tp.of_member
            && let Some(b) = self.param_named(&first.name)
        {
            let ty = match b.bound {
                Some((bound, arg)) => Ty::Param {
                    arg,
                    bound,
                    name: b.name.clone(),
                },
                None => Ty::Poison,
            };
            if !tp.args.is_empty() {
                self.error(
                    first.span,
                    format!(
                        "`{}` is a type parameter, which takes no type arguments",
                        first.name
                    ),
                );
                return Ty::Bad;
            }
            return ty;
        }
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
            Some(Binding::Variant(m, i, _)) => {
                let name = &self.modules[m].defs[i].name.name;
                self.error(
                    first.span,
                    format!("`{}` is a variant of `{name}`, not a type", first.name),
                );
                Ty::Bad
            }
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

    /// The enum of the prelude that the built-in `.qualification` of a part, and the
    /// `.qualification` of an environment, have the type of (ADR 0099).
    pub(crate) fn qualification_kind(&mut self) -> FieldKind {
        let (m, i) = self
            .prelude_def("Qualification")
            .expect("the prelude defines Qualification");
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
        self.params = None;
        self.next_args = None;
        self.param_insts.clear();
        self.param_args.clear();
        self.use_site = None;
        self.scope.clear();
        self.uf.clear();
        self.skipped.clear();
        self.table_pins.clear();
        self.stated.clear();
        self.unused_stmts.clear();
        self.touches.clear();
        self.defaults.clear();
        self.enum_ids.clear();
        self.bus_def_failed.clear();
        self.source_override = None;
        let def = self.modules[module].defs[index].clone();
        let name = def.name.name.clone();
        let root = self
            .instantiate(&def, (module, index), &name, None)
            .expect("the root is made inside nothing, so nothing repeats");
        self.model.root = root;
        self.resolve_defaults();
        match def.kind {
            DefKind::Board | DefKind::System => self.assign_pins(&name, true),
            DefKind::Module => self.assign_pins(&name, false),
            _ => {}
        }
        self.finalize_nets();
        self.apply_unused();
        self.skip_unstated_optional();
        self.model.uses = std::mem::take(&mut self.uses);
        self.model.provider_conflicts = std::mem::take(&mut self.provider_conflicts);
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
        // A module's type parameters stand for the arguments it was made with, or for their
        // bounds when it is checked on its own (ADR 0115).
        let args = self.next_args.take();
        let env = self.param_env(def_ref, args.as_deref());
        let type_name = match &args {
            Some(a) if env.is_some() => self.applied_name(def_ref, a),
            _ => def.name.name.clone(),
        };
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
            type_name,
            parent,
            children: Vec::new(),
            bridge: None,
            dc: None,
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
            standard: Vec::new(),
            placed_as: placed.as_ref().map(|(_, n)| n.clone()),
            placed_def: placed.map(|(d, _)| d),
        });
        // Every part has a field for who assembles it (ADR 0024): the board narrows it, the
        // catalog row may state a default, and the design stage reads it. It has one for the letters
        // of its reference designator too (ADR 0046): its kind states them, a user part or a
        // board may narrow it, and the design stage reads it.
        if kind == InstKind::Part {
            let assembly = self.assembly_mode();
            for (name, kind) in [("assembly", assembly), ("designator", FieldKind::Text)] {
                let field = self.model.add_field(id, name, kind);
                self.model.add_member(id, name, Member::Field(field));
            }
            // And two for the numbers of the catalog row it is (ADR 0122): the supplier's part
            // number and the manufacturer's. A design that narrows either holds a part that
            // picks from a catalog to the row with that number (a hand pick), before the lock.
            for name in HAND_PICK {
                let field = self.model.add_field(id, name, FieldKind::Text);
                self.model.add_member(id, name, Member::Field(field));
            }
            // And one for how tall its body stands above the board (ADR 0045, "Height
            // limits"): the part or the board states it, and the design stage reads it.
            // A part that does not has no height, which is not a height of zero.
            let field = self
                .model
                .add_field(id, "height", FieldKind::Quantity(Dimension::LENGTH));
            self.model.add_member(id, "height", Member::Field(field));
            // And two for the temperatures it works between (ADR 0099): the part or the board
            // states them, a catalog row may state them, and the design stage compares them with
            // the board's `.environment`. A part that states none has no range, which is not a
            // range of zero kelvin.
            for name in ["temp_min", "temp_max"] {
                let field =
                    self.model
                        .add_field(id, name, FieldKind::Quantity(Dimension::TEMPERATURE));
                self.model.add_member(id, name, Member::Field(field));
            }
            // And one for the qualification it claims (ADR 0099): the part or the board states
            // it, a catalog row may state it, and the design stage compares it with what the
            // board's `.environment` requires. A part that states none claims none, which is
            // not the same as claiming `Qualification::None`.
            let kind = self.qualification_kind();
            let field = self.model.add_field(id, "qualification", kind);
            self.model
                .add_member(id, "qualification", Member::Field(field));
            // And one for how far its pins stand out of the board, on the face opposite the
            // part, once they are cut short (ADR 0045 section 15): the board's
            // physical file states it for an instance, the kind may bound it. A part that
            // does not has no known protrusion, which is not a protrusion of zero.
            let field =
                self.model
                    .add_field(id, "pin_protrusion", FieldKind::Quantity(Dimension::LENGTH));
            self.model
                .add_member(id, "pin_protrusion", Member::Field(field));
            // And one for the STEP model it is drawn from (ADR 0108), of the prelude's
            // `StepModel`: the part or the board states it, and the design stage reads it.
            // A part that states no file is drawn with its generated shape.
            self.add_step_model(id);
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
        let saved_params = std::mem::replace(&mut self.params, env);
        let saved_support = std::mem::replace(&mut self.in_support, false);
        let saved_scope = std::mem::replace(&mut self.scope, vec![id]);
        // A definition's own text is circuit source, even when a physical file is what
        // asked for it to be made.
        let saved_phys = self.phys.take();
        // A part or module that provides generics has their members and statements first
        // (ADR 0068), each read in its own file.
        let generics = self.provided(def);
        // The standard generics it stands for: its own definition when that is one (a standard
        // generic is its own first provider, ADR 0068), and those it provides.
        let own = (def.kind == DefKind::Generic && self.modules[def_ref.0].std)
            .then(|| def.name.name.clone());
        self.model.instances[id.0].standard = own
            .into_iter()
            .chain(
                generics
                    .iter()
                    .filter(|(_, gref)| self.modules[gref.0].std)
                    .map(|(g, _)| g.name.name.clone()),
            )
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
        if kind == InstKind::Board {
            self.declare_default_environment(&def.items, def.span);
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
            self.finish_classes(id);
            self.finish_rules(id);
            self.finish_dc(id);
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
        self.params = saved_params;
        self.in_support = saved_support;
        self.scope = saved_scope;
        self.making.pop();
        id
    }

    /// Give a board the environment its project names (the manifest's `environment`, ADR 0099)
    /// when the board declares none: the member `.environment: Name`, made as if the board had
    /// written it, from the definition the manifest names. A board that declares an
    /// `.environment` of its own has that one alone, not the two together.
    fn declare_default_environment(&mut self, items: &[Item], span: Span) {
        let loader = self.loader;
        let Some((module, name)) = loader.default_environment() else {
            return;
        };
        let declared = items.iter().any(|i| {
            matches!(i, Item::Decl(d)
                if d.name.leading_dot
                    && d.name.segments.first().is_some_and(|s| s.name == "environment"))
        });
        if declared {
            return;
        }
        let path: Vec<String> = module.split("::").map(str::to_string).collect();
        let Some(m) = self.load_module(&path) else {
            self.error(
                span,
                format!(
                    "the manifest's `environment` names the module `{module}`, which is not loaded"
                ),
            );
            return;
        };
        let found = match self.modules[m].scope.get(name).cloned() {
            Some(Binding::Def(dm, di)) if self.modules[dm].defs[di].kind == DefKind::Interface => {
                Some((dm, di))
            }
            _ => None,
        };
        let Some((dm, di)) = found else {
            self.error(
                span,
                format!("the manifest's `environment` names `{name}`, which `{module}` does not define as an interface"),
            );
            return;
        };
        let Ok(file) = parse(&format!("board B {{ .environment: {name} }}")) else {
            return;
        };
        let synthetic = file.definitions[0].items.clone();
        let saved = self.enter_def((dm, di));
        self.declare(&synthetic);
        self.process(&synthetic);
        self.leave_def(saved);
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
    /// The built-in `.model` of a part (ADR 0108): an instance of the prelude's `StepModel`,
    /// made in the prelude's own file like any member of that type.
    fn add_step_model(&mut self, part: InstId) {
        let (m, i) = self
            .prelude_def("StepModel")
            .expect("the prelude defines StepModel");
        let def = self.modules[m].defs[i].clone();
        let saved = self.use_site.take();
        if let Some(model) = self.instantiate(&def, (m, i), "model", Some(part)) {
            self.model.add_member(part, "model", Member::Inst(model));
        }
        self.use_site = saved;
    }

    fn add_place(&mut self, owner: InstId, span: Span) {
        let source = self.model.instance(owner).source;
        let place = self.model.add_instance(Instance {
            name: "place".to_string(),
            kind: InstKind::Interface,
            type_name: "Place".to_string(),
            parent: Some(owner),
            children: Vec::new(),
            bridge: None,
            dc: None,
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
            standard: Vec::new(),
            placed_as: None,
            placed_def: None,
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

    /// Whether the statement being lowered is in the definition of the part it is about, a
    /// generic's or a project's own, and not in the block of a board or a module that uses
    /// the part (`.q: NMosfet { ... }`): the part's own checks and the design's requirements.
    pub(crate) fn in_own_definition(&self) -> bool {
        self.scope.last().is_some_and(|&i| {
            let inst = self.model.instance(i);
            inst.kind == InstKind::Part && inst.def == self.cur_def
        })
    }

    pub(crate) fn current(&self) -> InstId {
        *self
            .scope
            .last()
            .expect("a scope is always open during elaboration")
    }

    /// First pass: create every member, so later statements can name any of them.
    pub(crate) fn declare(&mut self, items: &[Item]) {
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
            if let Item::Support(support) = item {
                self.declare_support(support);
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
            // A part's own `.mpn: String` (or `.lcsc: String`) is the member every part has
            // (ADR 0122): the declaration says what is already so.
            if HAND_PICK.contains(&name.as_str())
                && d.body.is_none()
                && !d.ty.of_member
                && self.model.instance(cur).kind == InstKind::Part
                && matches!(self.model.member(cur, &name), Some(Member::Field(f)) if self.model.field(f).kind == FieldKind::Text)
                && matches!(self.resolve_type(&d.ty), Ty::Builtin(Builtin::Text))
            {
                continue;
            }
            if self.model.member(cur, &name).is_some() {
                let message = self.redeclared(cur, &name, d);
                self.error(d.name.span, message);
                self.skipped.insert(key);
                continue;
            }

            // A request for a pin by capability (`.status: .u.GPIO`) declares a pin of the board;
            // which pin of the part it gets is decided when the elaboration is done.
            let ty = if d.ty.of_member {
                Ty::Builtin(Builtin::Pin)
            } else {
                self.resolve_type(&d.ty)
            };
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
            if !self.fits_here(&ty, d) || !self.fits_support(&ty, d) {
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
            let optional = self.optional_ok(d, &ty);
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
                    if d.ty.of_member {
                        self.record_request(cur, pin, d);
                    }
                }
                Ty::Def(m, i) if self.misplaced(&self.modules[m].defs[i].clone(), d) => {
                    self.model.add_member(cur, &name, Member::Poison)
                }
                Ty::Param {
                    arg,
                    bound,
                    name: param,
                } => {
                    let last = d.ty.segments.last().map_or(d.name.span.end, |s| s.span.end);
                    let site = (self.cur_source(), Span::new(d.name.span.start, last));
                    self.use_site = Some(site);
                    let Some(child) = self.instantiate_param(&name, cur, arg, bound, &param) else {
                        self.model.add_member(cur, &name, Member::Poison);
                        continue;
                    };
                    self.model.instances[child.0].docs = d.docs.clone();
                    self.model.instances[child.0].declared = Some(site);
                    self.model.add_member(cur, &name, Member::Inst(child));
                }
                Ty::Def(m, i) => {
                    let def = self.modules[m].defs[i].clone();
                    // The parts a parametric module is made with (ADR 0115).
                    let Some(args) = self.type_arguments(&d.ty, (m, i)) else {
                        self.model.add_member(cur, &name, Member::Poison);
                        continue;
                    };
                    // Which arguments are this module's own parameters (ADR 0120).
                    let given: Vec<(String, Option<String>)> =
                        d.ty.args
                            .iter()
                            .zip(&def.params)
                            .map(|(arg, p)| {
                                let own = (arg.segments.len() == 1)
                                    .then(|| arg.segments[0].name.clone())
                                    .filter(|n| self.param_named(n).is_some());
                                (p.name.name.clone(), own)
                            })
                            .collect();
                    if !args.is_empty() {
                        self.next_args = Some(args);
                    }
                    let last = d.ty.segments.last().map_or(d.name.span.end, |s| s.span.end);
                    let site = (self.cur_source(), Span::new(d.name.span.start, last));
                    self.use_site = Some(site);
                    let made = self.instantiate(&def, (m, i), &name, Some(cur));
                    self.next_args = None;
                    let Some(child) = made else {
                        self.model.add_member(cur, &name, Member::Poison);
                        continue;
                    };
                    if !given.is_empty() {
                        self.param_args.insert(child, given);
                    }
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

            if optional {
                self.mark_optional(d);
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
            dc: None,
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
            standard: Vec::new(),
            placed_as: None,
            placed_def: None,
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
                Item::Support(support) => {
                    if self.support_allowed() {
                        let items = crate::support::wiring(support);
                        self.process(&items);
                    }
                }
                Item::Row(r) => self.row(r),
                Item::Polygon(p) => self.polygon(p),
                Item::Connect(c) if self.phys.is_none() => self.connect(c),
                Item::JoinMap(b) if self.phys.is_none() => self.join_map(b),
                Item::Bridge(b) if self.phys.is_none() => self.bridge(b),
                Item::Dc(d) if self.phys.is_none() => self.dc(d),
                Item::Catalog(c) if self.phys.is_none() => self.catalog(c),
                Item::Pins(p) if self.phys.is_none() => self.pins(p),
                Item::Tie(t) if self.phys.is_none() => self.tie(t),
                Item::Unused(u) if self.phys.is_none() => self.unused(u),
                Item::Peripheral(p) if self.phys.is_none() => self.peripheral(p),
                Item::Shared(s) if self.phys.is_none() => self.shared(s),
                Item::Lines(l) if self.phys.is_none() => self.lines(l),
                Item::Connect(c) => self.not_physical(c.span, "a connection with `~`"),
                Item::JoinMap(b) => self.not_physical(b.span, "a connection with `~`"),
                Item::Bridge(b) => self.not_physical(b.span, "a `bridge`"),
                Item::Dc(d) => self.not_physical(d.span, "a `dc` statement"),
                Item::Catalog(c) => self.not_physical(c.span, "a `catalog`"),
                Item::Pins(p) => self.not_physical(p.span, "a `pins` table"),
                Item::Tie(t) => self.not_physical(t.span, "a `tie`"),
                Item::Unused(u) => self.not_physical(u.span, "an `unused` statement"),
                Item::Peripheral(p) => self.not_physical(p.span, "a `peripheral`"),
                Item::Shared(s) => self.not_physical(s.span, "a `shared` resource"),
                Item::Lines(l) => self.not_physical(l.span, "a `lines` table"),
            }
        }
    }

    // ---- names ---------------------------------------------------------------

    /// Whether `owner` is the IC that a parametric module's user named (`.ic` of
    /// `Buck<Lt8641>`), reached from outside the module by a narrowing (ADR 0116) or a join
    /// (ADR 0120): then it is the part it is, not its parameter's bound.
    fn named_ic(&self, owner: InstId) -> bool {
        if !self.param_insts.contains_key(&owner) {
            return false;
        }
        let outside = self
            .model
            .instance(owner)
            .parent
            .is_some_and(|p| self.model.instance(p).def != self.cur_def);
        (self.narrowing && outside) || (self.joining && self.joined_through(owner))
    }

    /// Whether a join from the definition being read may enter `inst`: a member made from a
    /// type parameter (`.ic: Ic`), or a module given one (`.buck: Buck<Ic>`), whose parameter
    /// leads, through the modules around it, to a part this definition named (ADR 0120).
    pub(crate) fn joined_through(&self, inst: InstId) -> bool {
        let Some(parent) = self.model.instance(inst).parent else {
            return false;
        };
        if let Some(param) = self.param_insts.get(&inst) {
            return self.param_named_here(parent, param);
        }
        self.param_args.get(&inst).is_some_and(|given| {
            given
                .iter()
                .filter_map(|(_, own)| own.as_deref())
                .any(|own| self.param_named_here(parent, own))
        })
    }

    /// Whether the parameter `param` of the module `module` stands for a part that the
    /// definition being read named, directly (`.reg: Buck<Zbk2>`) or through the parameters of
    /// the modules between (`.reg: Converter<Zbk2>` holding `.buck: Buck<Ic>`).
    fn param_named_here(&self, module: InstId, param: &str) -> bool {
        let Some(given) = self.param_args.get(&module) else {
            return false;
        };
        let Some((_, own)) = given.iter().find(|(p, _)| p == param) else {
            return false;
        };
        let Some(parent) = self.model.instance(module).parent else {
            return false;
        };
        match own {
            // A part named where the module was placed: open only to that definition.
            None => self.model.instance(parent).def == self.cur_def,
            // The parameter of the module around it: follow it outward, unless that module is
            // the one being read, whose own text sees only the bound (ADR 0115).
            Some(outer) => {
                self.model.instance(parent).def != self.cur_def
                    && self.param_named_here(parent, outer)
            }
        }
    }

    /// Whether `member` of `owner` may be named from the definition being written.
    ///
    /// A module is configured through its interface: its rails, pins and values. The parts
    /// and modules inside it may be named from outside only on the left of a `&=`, which
    /// narrows a value inside it (ADR 0116), and in a join to a pin of the IC that the user
    /// named for a type parameter (ADR 0120).
    fn visible(&mut self, owner: InstId, member: Member, name: &netsmith_lang::ast::Ident) -> bool {
        // A board's physical file names a part inside a module by its path (ADR 0048). It
        // may only state physical intent about it: `physical_gate` refuses the rest.
        if self.phys.is_some() {
            return true;
        }
        // A narrowing from outside a parametric module reaches its IC as the part it is: the
        // board named it (`Buck<Lt8641>`), so its `support` parts are there (ADR 0116). A join
        // from outside reaches the named IC's own pins and interfaces (ADR 0120). The module's
        // own text still reaches only the bound's members.
        let named_ic = self.named_ic(owner);
        // A use of a generic reaches the members the generic declares, so that the board works
        // with whichever provider is picked (ADR 0068).
        if let Some(generic) = self
            .outside_contract(owner, &name.name)
            .filter(|_| !named_ic)
        {
            if self.report_outside_bound(owner, name) {
                return false;
            }
            let ty = self.model.instance(owner).type_name.clone();
            if self.joining && self.holds_type_param(owner) {
                self.error(
                    name.span,
                    format!(
                        "`{}` is not declared by `{generic}`: this use picks its IC through the generic, so the IC and its pins are not known here, and a join reaches the pins of a module's IC only where the IC is named, as in `{ty}`; name it as the type, or join what `{generic}` declares",
                        name.name
                    ),
                );
                return false;
            }
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
        // A join reaches the named IC's own pins and interfaces, not the parts of its `support`
        // section (ADR 0120).
        if named_ic && !self.narrowing && internal {
            let ic = inst.type_name.clone();
            let module = inst
                .parent
                .map(|p| self.model.instance(p).type_name.clone())
                .unwrap_or_default();
            self.error(
                name.span,
                format!(
                    "`{}` is a part of the `support` section of `{ic}`, inside `{module}`: a join reaches only the named IC's own pins and interfaces; narrow a value of `{}` with `&=`",
                    name.name, name.name
                ),
            );
            return false;
        }
        // The member a module's user named a part for (`.ic: Ic` of `Buck<Lt8641>`) is open to
        // a join, so its pins can be wired (ADR 0120).
        let named_param =
            matches!(member, Member::Inst(child) if self.joining && self.joined_through(child));
        // A module's user may narrow any member inside it by its path, and do nothing else
        // there (ADR 0116): `.stage.divider.rtop.resistance &= 100kohm +/- 1%`.
        if outside && internal && !self.narrowing && !named_param {
            let ty = inst.type_name.clone();
            // A member that holds the IC of a type parameter that no part was named for here.
            let carries = matches!(member, Member::Inst(child)
                if self.param_insts.contains_key(&child) || self.param_args.contains_key(&child));
            if self.joining && carries {
                self.error(
                    name.span,
                    format!(
                        "`{}` is inside `{ty}`, and the part it holds was not named here: a join reaches the pins of a module's IC only where its user names the part (`.reg: Buck<Lt8641>`), and a module's own text reaches only its parameter's bound",
                        name.name
                    ),
                );
                return false;
            }
            self.error(
                name.span,
                format!(
                    "`{}` is inside `{ty}` and is not part of its interface: only narrowing (`&=`) reaches inside a module; narrow a value inside it with `&=`, or use a rail, a pin, or a value that `{ty}` declares",
                    name.name
                ),
            );
            return false;
        }
        true
    }

    /// What to say when `name` is not a member of `inst` but names a group of it (ADR 0119):
    /// the group as a whole (`decoupling`), or an index past its last member (`decoupling[7]`).
    fn group_hint(&self, inst: InstId, name: &str) -> Option<String> {
        let (base, index) = match name.strip_suffix(']').and_then(|n| n.rsplit_once('[')) {
            Some((base, index)) => (base, Some(index)),
            None => (name, None),
        };
        let count = (0..)
            .take_while(|k| self.model.member(inst, &format!("{base}[{k}]")).is_some())
            .count();
        if count == 0 {
            return None;
        }
        let last = count - 1;
        Some(match index {
            None => format!(
                "`.{base}` is a group of {count}, not one member: name one, `.{base}[0]` to `.{base}[{last}]`; a statement in the group's block holds for every member"
            ),
            Some(index) => format!(
                "`.{base}` is a group of {count}, `.{base}[0]` to `.{base}[{last}]`, and has no member `[{index}]`"
            ),
        })
    }

    /// What to add to "has no member `count`" for a `Capacitor` (ADR 0123): a capacitor is one
    /// part, and several are a `CapacitorBank` or `N x Capacitor`. Empty for any other name or
    /// part.
    fn count_hint(&self, inst: InstId, name: &str) -> &'static str {
        if name == "count"
            && self
                .model
                .instance(inst)
                .standard
                .iter()
                .any(|g| g == "Capacitor")
        {
            ": a `Capacitor` is one part; for a count the solver chooses write a `CapacitorBank` \
             (`.bulk: CapacitorBank { .count &= 1..8 }`), and for a number you know write \
             `3 x Capacitor` (a group of identical members)"
        } else {
            ""
        }
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
                    if let Some(hint) = self.group_hint(cur, &first.name) {
                        self.error(first.span, hint);
                        return Res::Bad;
                    }
                    let owner = self.model.instance(cur).type_name.clone();
                    let names: Vec<String> = self
                        .model
                        .instance(cur)
                        .children
                        .iter()
                        .map(|(n, _)| n.clone())
                        .collect();
                    let near = crate::lower::closest(&first.name, &names)
                        .map(|n| format!("; did you mean `.{n}`?"))
                        .unwrap_or_default();
                    let hint = self.count_hint(cur, &first.name);
                    self.error(
                        first.span,
                        format!(
                            "`{}` is not a member of this {owner}{near}{hint}",
                            first.name
                        ),
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
                    } else if p.segments.len() == 1
                        && let Some(spellings) = self.variant_hint(&first.name)
                    {
                        // A variant is in scope by its enum or by its own import (ADR 0118).
                        format!("; did you mean {spellings}?")
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
                        if !self.named_ic(i) && self.report_outside_bound(i, seg) {
                            return Res::Bad;
                        }
                        if let Some(hint) = self.group_hint(i, &seg.name) {
                            self.error(seg.span, hint);
                            return Res::Bad;
                        }
                        let ty = self.model.instance(i).type_name.clone();
                        let hint = self.count_hint(i, &seg.name);
                        self.error(
                            seg.span,
                            format!("`{ty}` has no member `{}`{hint}", seg.name),
                        );
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
/// outline, cutouts, mounting holes, fiducials, labels and case models (ADR 0109) belong in
/// its `case` section;
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
            (DefKind::Interface, "CaseModel") => ("a case model", true),
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
                            "a physical file declares the outline, cutouts, keepouts, mounting holes, fiducials, labels and case models of a board; `{member}: {type_name}` is none of them, and a part or a module belongs in the circuit source"
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
    /// compare only physical things: a part's `.place`, a part's `.height`, `.pin_protrusion` and `.model`, and
    /// what the file declared itself. A board's circuit source may mention none of a `.place`
    /// (ADR 0048 decision 10). Reports it when not.
    pub(crate) fn physical_gate(&mut self, fields: &[crate::model::FieldId], span: Span) -> bool {
        if let Some(phys) = &self.phys {
            for &f in fields {
                if !self.physical_field(f, phys) {
                    let path = self.model.path_of_field(f);
                    self.error(
                        span,
                        format!(
                            "`{path}` is not physical intent: a physical file states where a part is (its `.place`), how tall it is (`.height`), how far its pins protrude (`.pin_protrusion`), the STEP model it is drawn from (`.model`) and the outline, cutouts, keepouts, holes, fiducials, labels and case models it declares, never what a part is; put this in the circuit source"
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
        if self.model.is_step_model(f.owner) {
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
