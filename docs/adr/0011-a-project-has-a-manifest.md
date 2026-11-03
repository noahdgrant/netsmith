---
status: accepted
---

# A project is a directory with a `netsmith.toml`

A netsmith project is a directory with a `netsmith.toml` at its root, found the way cargo finds `Cargo.toml`: the commands look in the current directory and then each parent. `netsmith init`, run in a directory, makes one there (an existing or new git repository; netsmith does not run git, and says so when the directory is not in one). There is no `new` command, so there is one way to start. A file with no manifest still works as before: `netsmith build led.nsm` needs no project.

The manifest has one table, `[project]`, and only `name` is required:

```toml
[project]
name = "demo"
top_level = ["src/main.nsm"]
out = "out"
fab = "fab"
kicad = 9
```

`top_level` lists the files that hold boards, and `netsmith build`, `fab` and `editor` with no file act on them (`view`, which they once named, is replaced by `editor`, ADR 0019), from anywhere under the root. We chose a list over a convention that every file in `src/` is built, because a library file of modules builds nothing, and "which files are the project" is then written once, in the one place a reader looks. When a `system` of several boards exists, its file is listed here the same way; the key does not change. A project that is only a library of modules lists none (`top_level = []`): `build` and `fab` with no file then stop with "this project has no boards", and a named file still builds with the project's modules in reach. `out` and `fab` are where the commands write, relative to the root, and `kicad` is the version of the files written. netsmith writes KiCad 9 only, so any other number is refused naming the field, rather than accepted and ignored. A path in the manifest stays inside the project (relative, no `..`).

Other files are modules. The module path is the file's path under the root: `src/power.nsm` is `src::power`, `src/parts/status.nsm` is `src::parts::status`, and `import src::power::Rails` reads like an import from the standard library. This needs nothing new in the loader: the project's `src/` files are inserted into it next to the embedded standard library (`Loader::std()`, which stays embedded in the binary). A file name that cannot be written in an import is refused, naming the file, rather than skipped. Imports of a file that is not there are the usual "unknown module" error. A named file inside a project sees the project's modules, but keeps `out` and `fab` relative to where it is run, as it always did; only the commands with no file use the manifest's directories. A manifest that cannot be read stops the commands of its project, with its line.

Dependencies on other projects have no place in the manifest yet, and none is reserved. They would be a `[dependencies]` table beside `[project]`, naming a project by path and mapping to a module root next to `src`; the loader would take one more prefix. That is a later decision, made with the first need for it.

`netsmith init` writes `netsmith.toml`, `src/main.nsm`, `.gitignore` and `README.md` (updated by ADR 0015: it also writes `AGENTS.md`, the user skills and the language guide). The starter board is `examples/led.nsm` itself, compiled in with `include_str!` and renamed after the project, so the example and the scaffold are one text, and a test builds and fabs the scaffold. It never overwrites: if `netsmith.toml` or `src/main.nsm` exists it writes nothing and lists them. An existing `README.md` is kept, and an existing `.gitignore` gains the `/out/` and `/fab/` lines it lacks. It refuses inside another project, because the nearest manifest wins and the inner project would never be reached from the outer.

The manifest is TOML, read and written with the `toml` crate through serde (`crates/netsmith/src/project.rs`); netsmith does not parse TOML itself. `Manifest` is a serde struct inside a `[project]` table, with `deny_unknown_fields` on both, so an unknown key or another table is refused with the crate's own error, which names the line, column and key. A value of the wrong type is refused the same way. What the types cannot say (the supported KiCad version, paths inside the project, `out` and `fab` differing) is checked afterwards and names the field. `init` writes the file with the same serialization, so what it writes is what is read back.
