//! Where `import` paths come from. The standard library is embedded in the binary.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Loader {
    /// `std::parts::passive` to (display name, source text).
    modules: HashMap<String, (String, String)>,
    /// A source file's name (as `check` is given it, or as `insert` names a module) to the
    /// physical file beside it: its display name and text (ADR 0048).
    physical: HashMap<String, (String, String)>,
    /// A top-level file's name as it was typed to the module key it has in the project
    /// (`./src/main.nsm` and `/home/me/proj/src/main.nsm` to `src::main`), for a name that
    /// the file names of the modules do not match as they are.
    root_keys: HashMap<String, String>,
}

impl Loader {
    pub fn empty() -> Self {
        Self::default()
    }

    /// A loader holding the standard library.
    pub fn std() -> Self {
        let mut loader = Self::empty();
        // The prelude is in scope in every file with no import; it is listed here so that
        // tools that show the standard library show it too.
        loader.insert(
            "std::prelude",
            "std/prelude.nsm",
            include_str!("../../../std/prelude.nsm"),
        );
        loader.insert(
            "std::parts::passive",
            "std/parts/passive.nsm",
            include_str!("../../../std/parts/passive.nsm"),
        );
        loader.insert(
            "std::parts::connector",
            "std/parts/connector.nsm",
            include_str!("../../../std/parts/connector.nsm"),
        );
        loader.insert(
            "std::parts::regulator",
            "std/parts/regulator.nsm",
            include_str!("../../../std/parts/regulator.nsm"),
        );
        loader.insert(
            "std::parts::logic",
            "std/parts/logic.nsm",
            include_str!("../../../std/parts/logic.nsm"),
        );
        loader.insert(
            "std::parts::diode",
            "std/parts/diode.nsm",
            include_str!("../../../std/parts/diode.nsm"),
        );
        loader.insert(
            "std::parts::transistor",
            "std/parts/transistor.nsm",
            include_str!("../../../std/parts/transistor.nsm"),
        );
        loader.insert(
            "std::parts::mechanical",
            "std/parts/mechanical.nsm",
            include_str!("../../../std/parts/mechanical.nsm"),
        );
        loader.insert(
            "std::pcb",
            "std/pcb.nsm",
            include_str!("../../../std/pcb.nsm"),
        );
        loader.insert(
            "std::bus",
            "std/bus.nsm",
            include_str!("../../../std/bus.nsm"),
        );
        loader.insert(
            "std::mfg::stackup",
            "std/mfg/stackup.nsm",
            include_str!("../../../std/mfg/stackup.nsm"),
        );
        loader.insert(
            "std::mfg::jlcpcb",
            "std/mfg/jlcpcb.nsm",
            include_str!("../../../std/mfg/jlcpcb.nsm"),
        );
        loader.insert(
            "std::sources",
            "std/sources.nsm",
            include_str!("../../../std/sources.nsm"),
        );
        loader.insert(
            "std::logic",
            "std/logic.nsm",
            include_str!("../../../std/logic.nsm"),
        );
        loader
    }

    pub fn insert(&mut self, module: &str, file_name: &str, text: &str) {
        self.modules.insert(
            module.to_string(),
            (file_name.to_string(), text.to_string()),
        );
    }

    /// Pair the physical file `file_name` (text `text`) with the source file called
    /// `source_name`: the name `check` is given for the top-level file, or the file name
    /// `insert` gave a module. The checker puts a unit `physical for Board` with the board
    /// of that name in that file (ADR 0048).
    pub fn attach_physical(&mut self, source_name: &str, file_name: &str, text: &str) {
        self.physical.insert(
            source_name.to_string(),
            (file_name.to_string(), text.to_string()),
        );
    }

    /// [`Loader::attach_physical`] on a loader that is being built.
    pub fn with_physical(mut self, source_name: &str, file_name: &str, text: &str) -> Self {
        self.attach_physical(source_name, file_name, text);
        self
    }

    /// Say that the top-level file typed as `file_name` is the module `key` of the project
    /// (`src::main`): whatever spelling a tool was given for it, the file is one module, held
    /// once, and its providers sort under the same module path (ADR 0059 sections 4 and 11).
    pub fn attach_root_key(&mut self, file_name: &str, key: &str) {
        self.root_keys
            .insert(file_name.to_string(), key.to_string());
    }

    /// The module key of the top-level file `file_name` when it is one of the loaded modules:
    /// the key attached to this spelling, or the module whose file name is this one with its
    /// `.` and `..` steps taken out (`./src/main.nsm` is `src/main.nsm`). `None` for a file
    /// that no module is.
    pub(crate) fn root_key(&self, file_name: &str) -> Option<String> {
        if let Some(key) = self.root_keys.get(file_name) {
            return Some(key.clone());
        }
        let wanted = normalise_name(file_name);
        self.modules
            .iter()
            .find(|(_, (name, _))| normalise_name(name) == wanted)
            .map(|(key, _)| key.clone())
    }

    /// The physical file paired with a source file: its name and text.
    pub(crate) fn physical_of(&self, source_name: &str) -> Option<&(String, String)> {
        self.physical.get(source_name)
    }

    /// Every module, sorted by name: its name (`std::sources`), the file name it is shown
    /// as in messages (`std/sources.nsm`) and its text.
    pub fn modules(&self) -> Vec<(&str, &str, &str)> {
        let mut all: Vec<_> = self
            .modules
            .iter()
            .map(|(key, (file, text))| (key.as_str(), file.as_str(), text.as_str()))
            .collect();
        all.sort();
        all
    }

    pub(crate) fn get(&self, module: &str) -> Option<&(String, String)> {
        self.modules.get(module)
    }
}

/// A file name without the `.` steps, the empty steps and the `..` steps that undo a step
/// before them: the same file spelled two ways has one normal form (`./src/../src/main.nsm`
/// is `src/main.nsm`).
pub(crate) fn normalise_name(name: &str) -> String {
    let mut steps: Vec<&str> = Vec::new();
    for step in name.split('/') {
        match step {
            "" | "." => {}
            ".." if steps.last().is_some_and(|last| *last != "..") => {
                steps.pop();
            }
            step => steps.push(step),
        }
    }
    let absolute = if name.starts_with('/') { "/" } else { "" };
    format!("{absolute}{}", steps.join("/"))
}
