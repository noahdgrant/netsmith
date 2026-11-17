//! The linter (issue #1104): what is true of a source that checks and still is worth changing.
//! The checker says what is wrong; the guideline findings judge a design against engineering
//! guidelines (ADR 0036); the linter judges the source as a source. It reads a project that
//! checks, never changes a file, and says each finding with its rule, the file, the line and the
//! column, and what to change.
//!
//! The rules (the owner's choice of four, issue #1104), each with its page in the guide:
//!
//! - [`Rule::UnusedImport`]: a name an `import` brings in that the file never uses
//!   (`imports.rs`; the guide's chapter on imports).
//! - [`Rule::UnusedDefinition`]: a definition no board of the project reaches, and a member of a
//!   module or a board that is never joined or used (`unused.rs`; the chapter on definitions).
//! - [`Rule::Redundant`]: a narrowing or a default that a default of the kind, of a module round
//!   it or of the board's `defaults` block, or the catalog row the part is picked by, already
//!   gives, and an attribute that a generic the part provides, the row, the manifest or the
//!   language's own default already gives (`redundant.rs`; the chapter on narrowing).
//! - [`Rule::Naming`]: a name that breaks the convention the guide teaches, `CamelCase`
//!   definitions and `snake_case` members (`naming.rs`; the chapter on definitions).
//!
//! A rule is turned off for a project by the manifest (`[lint] off = ["naming"]`, read by the
//! command line) and for one line by a comment, `// lint: off naming` at the end of the line or
//! alone on the line above it ([`suppress`]).
//!
//! The linter checks the top-level files as `check` does, and refuses to lint a project that
//! does not check: the checker's findings stay the checker's.

mod imports;
mod naming;
mod redundant;
pub mod suppress;
mod unused;

use std::collections::{BTreeMap, BTreeSet};

use netsmith_check::{Assignment, Checked, Loader, check_with};
use netsmith_lang::Span;
use netsmith_lang::ast;
use netsmith_solve::Catalogs;

/// One rule of the linter. Its name is what the manifest and a line's comment turn off, and
/// `lint.` and its name is the code of its findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    UnusedImport,
    UnusedDefinition,
    Redundant,
    Naming,
}

impl Rule {
    pub const ALL: [Rule; 4] = [
        Rule::UnusedImport,
        Rule::UnusedDefinition,
        Rule::Redundant,
        Rule::Naming,
    ];

    /// The rule's name: `unused_import`, `unused_definition`, `redundant`, `naming`.
    pub fn name(self) -> &'static str {
        match self {
            Rule::UnusedImport => "unused_import",
            Rule::UnusedDefinition => "unused_definition",
            Rule::Redundant => "redundant",
            Rule::Naming => "naming",
        }
    }

    /// The code of its findings: `lint.` and its name.
    pub fn code(self) -> String {
        format!("lint.{}", self.name())
    }

    pub fn from_name(name: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|r| r.name() == name)
    }

    /// The names of every rule, for a message that refuses another.
    pub fn names() -> String {
        Rule::ALL
            .iter()
            .map(|r| format!("`{}`", r.name()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// One finding of the linter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lint {
    pub rule: Rule,
    /// The file as the checker names it (`src/power.nsm`, `std/parts/passive.nsm`).
    pub file: String,
    /// 1-based.
    pub line: usize,
    /// 1-based, in characters.
    pub column: usize,
    /// What the finding is and what to change.
    pub message: String,
}

impl Lint {
    /// `file:line:column: lint.rule: message`, as the command line prints it after
    /// `warning: ` or `error: `.
    pub fn render(&self) -> String {
        format!(
            "{}:{}:{}: {}: {}",
            self.file,
            self.line,
            self.column,
            self.rule.code(),
            self.message
        )
    }
}

/// A top-level file: one the checker is run on, as `check` is.
#[derive(Debug, Clone)]
pub struct Root {
    /// What the checker calls it (`src/main.nsm`).
    pub name: String,
    pub text: String,
    /// The providers of the generics' uses that the file's pick lock holds (ADR 0113), as the
    /// build checks it with; empty for none.
    pub assignment: Assignment,
}

/// Whether a file checks as the build takes it: no error, or only contradictions inside uses
/// that have several providers, which the build tries the other providers for (ADR 0113).
pub(crate) fn checks(checked: &Checked) -> bool {
    if checked.is_ok() {
        return true;
    }
    let conflicts: Vec<&netsmith_check::Diagnostic> = checked
        .boards
        .iter()
        .map(|b| &b.model)
        .chain(checked.systems.iter().map(|s| &s.model))
        .flat_map(|m| &m.provider_conflicts)
        .collect();
    !conflicts.is_empty()
        && checked.syntax.is_empty()
        && checked.diagnostics.iter().all(|d| conflicts.contains(&d))
}

/// What the linter reads.
pub struct Project<'a> {
    /// The standard library, the project's modules and its dependencies', with the physical
    /// files attached, as the commands make it.
    pub loader: &'a Loader,
    /// The top-level files, each checked.
    pub roots: Vec<Root>,
    /// Whether the files of the project are the whole project (`netsmith project lint` with no
    /// file): only then is a definition that no board reaches unused. Linting one file of a
    /// project, a definition of it may be for a board in another.
    pub whole: bool,
    /// The catalogs, for a narrowing that the row a part is picked by gives. With none, that
    /// half of [`Rule::Redundant`] says nothing.
    pub catalogs: Option<&'a Catalogs>,
    /// The rules turned off.
    pub off: BTreeSet<Rule>,
    /// The definitions the manifest names (its `environment`), which no source names and a
    /// board still reaches.
    pub named: Vec<String>,
}

/// Why the linter did not lint: a file does not check or does not parse (the checker says why,
/// `netsmith board check`), or a line's comment turns off a rule that does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

impl Refusal {
    fn does_not_check(file: &str, errors: usize) -> Refusal {
        Refusal(format!(
            "{file} does not check ({errors} error{}): the linter reads a source that checks; `netsmith board check` says what is wrong",
            if errors == 1 { "" } else { "s" }
        ))
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What kind of file of the project a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    /// A top-level file, checked.
    Root,
    /// A module of the project (`src::power`), with its key in the loader.
    Module,
    /// A physical file beside a source.
    Physical,
}

/// One file of the project, parsed.
pub(crate) struct SourceFile {
    pub name: String,
    pub text: String,
    pub kind: FileKind,
    /// The module key, for a module (`src::power`).
    pub key: Option<String>,
    /// For a physical file: the name of the source it is beside.
    pub beside: Option<String>,
    pub ast: ast::File,
}

impl SourceFile {
    /// 1-based line and column of a byte offset.
    pub fn line_col(&self, at: usize) -> (usize, usize) {
        line_col(&self.text, at)
    }

    pub fn lint(&self, rule: Rule, span: Span, message: String) -> Lint {
        let (line, column) = self.line_col(span.start);
        Lint {
            rule,
            file: self.name.clone(),
            line,
            column,
            message,
        }
    }
}

/// 1-based line and column (in characters) of a byte offset of `text`.
pub(crate) fn line_col(text: &str, at: usize) -> (usize, usize) {
    let at = netsmith_lang::floor_boundary(text, at);
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}

/// What every rule reads: the project's files and the checked top-level files.
pub(crate) struct Context<'a> {
    pub project: &'a Project<'a>,
    /// Every file of the project, by name: the top-level files, the modules under `src`, and
    /// the physical files beside them. The standard library and the dependencies are not.
    pub files: BTreeMap<String, SourceFile>,
    /// Each top-level file as the checker leaves it.
    pub checked: Vec<(String, Checked)>,
}

impl Context<'_> {
    /// Each model of a checked file: the boards', the systems' and the panels'.
    pub fn models(&self) -> impl Iterator<Item = (&Checked, &netsmith_check::Model)> {
        self.checked.iter().flat_map(|(_, c)| {
            c.boards
                .iter()
                .map(|b| &b.model)
                .chain(c.systems.iter().map(|s| &s.model))
                .chain(c.panels.iter().map(|p| &p.model))
                .map(move |m| (c, m))
        })
    }
}

/// A file name without a leading `./`, so that one file spelled two ways is one.
fn plain(name: &str) -> &str {
    name.strip_prefix("./").unwrap_or(name)
}

/// Lint a project: the findings of every rule that is on, in the files of the project, each
/// line's comments obeyed, by file, line and column.
pub fn lint(project: &Project) -> Result<Vec<Lint>, Refusal> {
    let mut files: BTreeMap<String, SourceFile> = BTreeMap::new();
    let add = |files: &mut BTreeMap<String, SourceFile>,
               name: &str,
               text: &str,
               kind: FileKind,
               key: Option<String>,
               beside: Option<String>|
     -> Result<(), Refusal> {
        let ast = netsmith_lang::parse(text).map_err(|_| Refusal::does_not_check(name, 1))?;
        if let Some((line, word)) = suppress::unknown(text).first() {
            return Err(Refusal(format!(
                "{name}:{line}: `// lint: off {word}` names no rule of the linter; the rules are {}",
                Rule::names()
            )));
        }
        files.insert(
            name.to_string(),
            SourceFile {
                name: name.to_string(),
                text: text.to_string(),
                kind,
                key,
                beside,
                ast,
            },
        );
        Ok(())
    };
    let roots: BTreeSet<&str> = project.roots.iter().map(|r| plain(&r.name)).collect();
    for root in &project.roots {
        add(
            &mut files,
            &root.name,
            &root.text,
            FileKind::Root,
            None,
            None,
        )?;
    }
    for (key, name, text) in project.loader.modules() {
        if !key.starts_with("src::") || roots.contains(plain(name)) {
            continue;
        }
        if project.whole {
            add(
                &mut files,
                name,
                text,
                FileKind::Module,
                Some(key.to_string()),
                None,
            )?;
        }
    }
    let sources: Vec<String> = files.keys().cloned().collect();
    for source in sources {
        if let Some((name, text)) = project.loader.physical_of(&source) {
            add(
                &mut files,
                &name.clone(),
                &text.clone(),
                FileKind::Physical,
                None,
                Some(source),
            )?;
        }
    }

    let mut checked = Vec::new();
    for root in &project.roots {
        let c = check_with(&root.name, &root.text, project.loader, &root.assignment);
        if !checks(&c) {
            let mut refusal =
                Refusal::does_not_check(&root.name, c.diagnostics.len() + c.syntax.len());
            refusal.0.push_str(&format!(":\n{}", c.render().trim_end()));
            return Err(refusal);
        }
        checked.push((root.name.clone(), c));
    }
    let cx = Context {
        project,
        files,
        checked,
    };

    let on = |rule: Rule| !project.off.contains(&rule);
    let mut found = Vec::new();
    if on(Rule::Naming) {
        found.extend(naming::lint(&cx));
    }
    if on(Rule::UnusedImport) {
        found.extend(imports::lint(&cx));
    }
    if on(Rule::UnusedDefinition) {
        found.extend(unused::lint(&cx));
    }
    if on(Rule::Redundant) {
        found.extend(redundant::lint(&cx));
    }
    found.retain(|l| {
        cx.files
            .get(&l.file)
            .is_none_or(|f| !suppress::is_off(&f.text, l.line, l.rule))
    });
    found.sort_by(|a, b| {
        (&a.file, a.line, a.column, a.rule).cmp(&(&b.file, b.line, b.column, b.rule))
    });
    found.dedup();
    Ok(found)
}

/// Lint files on their own, with no project round them: the rules that read one file
/// (naming, unused imports). What the tests of the standard library run on each of its files.
pub fn lint_file(name: &str, text: &str, loader: &Loader) -> Result<Vec<Lint>, Refusal> {
    let ast = netsmith_lang::parse(text).map_err(|_| Refusal::does_not_check(name, 1))?;
    let project = Project {
        loader,
        roots: Vec::new(),
        whole: false,
        catalogs: None,
        off: BTreeSet::new(),
        named: Vec::new(),
    };
    let mut files = BTreeMap::new();
    files.insert(
        name.to_string(),
        SourceFile {
            name: name.to_string(),
            text: text.to_string(),
            kind: FileKind::Module,
            key: None,
            beside: None,
            ast,
        },
    );
    let cx = Context {
        project: &project,
        files,
        checked: Vec::new(),
    };
    let mut found = naming::lint(&cx);
    found.extend(imports::lint(&cx));
    found.retain(|l| !suppress::is_off(text, l.line, l.rule));
    found.sort_by_key(|l| (l.line, l.column, l.rule));
    Ok(found)
}
