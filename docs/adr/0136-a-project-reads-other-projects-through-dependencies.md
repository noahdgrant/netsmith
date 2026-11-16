---
status: proposed
---

# A project reads other projects through `[dependencies]`, each under the name of its entry

Issue #68, the decision ADR 0011 left open. The owner chose the shape (2026-11-15): a `[dependencies]` table beside `[project]`, an entry names a project by path, git URL with a revision, or version, its `src/` is a module root next to `src` (`import shared::power::Rails`), its `catalogs/` are read too, and fetched dependencies have a lock of the resolved revisions beside `netsmith.toml`, checked like the pick lock (ADR 0100). The rules below fill in what that left open; they were chosen by the implementing agent while the owner was away, and wait for the owner.

**Decision.**

- An entry is `name = { path = "../shared" }`. The key, not the other project's `name`, is the first step of every import of its modules: `../shared/src/power.nsm` is `shared::power`. A key is a name an import can start with, and not `src` or `std`. `path` is relative to the root, `..` allowed (a dependency is outside the project by nature), never absolute, and must be a folder with a `netsmith.toml`.
- Inside a file of a dependency, `import src::x` is that project's own `src/x.nsm`, so a project builds the same alone and as a dependency. Every other import means what it means everywhere.
- A dependency's own dependencies are read too, and all of them share one set of names: two different projects under one name are refused, naming both folders; one project reached twice under one name is read once. A project that depends on itself through others is refused with the chain. Messages name a dependency's files from the depending root (`../shared/src/power.nsm`).
- A dependency gives its modules and its catalog files (its `catalog_dirs` and `catalogs`) and nothing else: its `top_level` boards are not built, and its `out`, `fab`, `preferred.csv`, orders, `[supply]`, `kicad.toml` and other keys are not read. A catalog that the project and a dependency (or two dependencies) both hold is refused naming both files, as two files of one catalog in one project are.
- `git`, `rev` and `version` are keys of an entry, and are refused with a message that says only `path` is built, so a manifest written for the later form fails plainly rather than as an unknown key.

**Rejected.**

- *The other project's `name` as the import prefix.* A project name may hold `-`, which an import cannot, and two projects may share a name; the key is chosen by the one who imports.
- *Rewriting the dependency's `src::` imports as text, or asking it to write `shared::` for its own files.* The second breaks the dependency on its own, and the first moves spans in messages.
- *Only direct dependencies in scope.* Scoping a name to the file's own project is a second resolution rule in the checker for a case nobody has; one flat set, with clashes refused, is what a reader sees in one look at the manifests.
- *Reading a dependency's `kicad.toml`, `preferred.csv` or `[supply]`.* Each needs a rule for when two projects disagree; they wait for a need.

**Cost.** One more name for each dependency in the import space, so a project cannot name a dependency `src` or `std`. A dependency's module that names a part needing a footprint mapping of the dependency's `kicad.toml` builds only where the depending project maps it too. Every build reads every dependency's manifest and `src/`.

**Left open.** Git and version dependencies, their cache and the lock of resolved revisions; a dependency's `kicad.toml` and libraries; `environment = ` naming a dependency's definition.
