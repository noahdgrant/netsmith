---
status: proposed
---

# A dependency by git is read at the commit `dependencies.lock.toml` holds

Issue #1012, what ADR 0136 left open. The owner chose the shape on 2026-11-15 (an entry names a project by path, git URL with a revision, or version; a lock of the resolved revisions beside `netsmith.toml`, checked like the pick lock, ADR 0100). The rules below were chosen by the implementing agent while the owner was away, and wait for the owner.

**Decision.**

- An entry is `{ path }`, `{ git, rev }` or `{ git, version }`. `git` is a URL or an absolute path, and its repository's top folder is the project. `rev` is a branch, a tag or a commit. There is no registry, so `version` is a requirement in cargo's syntax over the repository's tags (`v1.2.3` or `1.2.3`), the newest admitted one being read. Any other mix is refused, naming the entry and the forms.
- netsmith runs the `git` command, as the editor's comparison does (ADR 0059). Repositories are cached once per machine under `git/` of the cache folder of the supplier cache (`NETSMITH_CACHE_DIR`, `$XDG_CACHE_HOME/netsmith`, `~/.cache/netsmith`): a bare repository per URL (`git/db/<16 hex digits of the URL's SHA-256>`) and the files of each commit read (`git/checkouts/<same>/<commit>`), made beside their place and renamed into it.
- The lock is `dependencies.lock.toml` beside `netsmith.toml`: TOML, a `version`, and one `[[dependency]]` per dependency by git of the project and of its dependencies, with `name`, `git`, `rev` or `version`, and the full `commit`. A dependency's own lock is not read.
- As the pick lock: `build` and `fab` write it when it does not exist and never rewrite it; `lock --update` fetches each again and rewrites it, printing what changed; `lock --check` fails when an entry is missing or no manifest asks for it. A build reads the locked commit while the entry still says what the manifest says, and resolves again (writing nothing) when it does not.
- A build fetches only what the cache cannot answer: a commit already checked out runs no git command, and a `rev` or `version` with no lock entry is answered from the cached repository when it can. `--offline` turns a fetch into an error.
- `lock --check` fetches nothing: it compares the lock with the manifests, so CI needs no network. A dependency's manifest that the cache does not hold is not read, so an entry for its dependencies is not called extra.
- A file of a dependency by git is named in messages and in the editor as `<key>@<12 digits of the commit>/src/...`. A dependency by git names its own dependencies by git, or by a `path` that stays inside its repository.
- A dependency's `kicad.toml` is read with the project's (ADR 0021): its rows and libraries are added, and a row or library name in two files is refused naming both, as for catalogs. `environment = ` may name a dependency's definition by its module path, `shared::env::Lab`.

**Rejected.**

- *A git library (libgit2, gitoxide).* A large dependency for four commands, as ADR 0059 found.
- *A cache inside the project.* Every project would fetch the same repository again; the supplier cache is already per machine.
- *`lock --check` fetching to see whether a branch moved.* It needs the network in CI, and fails on someone else's push; moving forward is `lock --update`, reviewed as a diff.
- *A lock entry per depending project, or reading the dependency's own lock.* One flat set of names (ADR 0136) has one commit per name.
- *A version requirement over a registry.* There is none to ask.

**Cost.** git must be installed for a dependency by git. A first build fetches. A branch read once stays put on that machine until `lock --update`, also with no lock file. A message names a file in the cache by key and commit, not by a path that opens.

**Left open.** A registry, a subfolder of a repository as the project, and a pruning of the cache.
