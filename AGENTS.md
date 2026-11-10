# netsmith

A language and tools for designing electronics as code. `README.md` says what it is and how to run it.

## Build, test, lint

- Test with `cargo test --workspace`, lint with `cargo clippy --workspace --all-targets`, and check formatting with `cargo fmt --all --check`. Each passes with no warnings. CI runs fmt, clippy and the tests on every pull request, and the editor's browser recipes (`scripts/editor-verify-all`) in a `recipes` job. The `test`, `kicad` (the suite with KiCad 9 installed) and `recipes` jobs are slices that run side by side (4, 3 and 3); `scripts/ci-test-slices` and `scripts/ci-recipe-slices` prove that every test and every recipe runs in exactly one slice, and `scripts/ci-recipe-slices` says how to rebalance the recipes. A pull request runs only the jobs its change needs (`scripts/ci-changes` has the rules and `scripts/ci-changes-test` pins them, in the `changes` job; a push to main runs everything), and `ci-ok` is the one check to require. The `no-kicad` job (a runner with no KiCad) runs `scripts/check-no-kicad.sh`, which builds and fabs every example with KiCad hidden: `build`, `fab` and the editor need no KiCad install (ADR 0097).
- A test build is heavy: in a crate with no `tests/main.rs` every file of its `tests/` folder is its own executable (#190: every crate with a `tests/` folder has one executable, `tests/main.rs`, with the files as modules: a new file needs a `mod` line there, and one file's tests run with `cargo test -p netsmith --test main guide::`). The dev profile keeps line tables only (see `Cargo.toml`; `CARGO_PROFILE_DEV_DEBUG=2` brings the full debug information back for a debugger, `=0` drops it for the smallest build). Where memory is short, or several builds run at once, limit the linkers with `CARGO_BUILD_JOBS=3` and take turns with `flock /tmp/netsmith-heavy.lock cargo test ...` (one heavy build at a time; a machine ran out of memory with three).
- The commands are in `netsmith --help`; `cargo run -p netsmith -- build examples/led.nsm --out out` builds a sample board.
- The editor's page is committed as a built bundle in `crates/netsmith-editor/web-dist`. Only a change to its sources in `crates/netsmith-editor/web` needs Node 20 or newer: run `scripts/web-bundle update` and commit the bundle.

## Tests

- A test that needs a folder takes it from `netsmith_kicad::testlib::TempDir` (or `TempFile`), which removes it when the test ends, also on a panic. Bind the guard to a name, never to `_`. `NETSMITH_KEEP_TEST_DIRS=1` keeps the folders of a failing run.
- `docs/LANGUAGE.md` is the language guide, and `crates/netsmith/tests/guide.rs` runs its examples. Change the guide with the language.

## Data

A part number goes into a catalog only with a recorded source (`data/jlcpcb/README.md`).

## Vocabulary and decisions

- Use the terms in `GLOSSARY.md` and avoid the ones listed under _Avoid_.
- `docs/adr/README.md` says what an ADR is and is not (one page, no stage plans or progress, no amendments); `scripts/adr-check` enforces the size and the status words.
- Decisions that are hard to reverse are in `docs/adr/`; the language's design rules are in `docs/DESIGN-RULES.md`.
- Language-design choices (syntax, semantics) are the owner's. Present options and ask.
- Open work is in GitHub issues.

## Git

Stage files by explicit path when committing, so other people's in-progress files stay out.
Write commit messages (and pull request titles) as [Conventional Commits](https://www.conventionalcommits.org): `type(scope): summary`, with a type such as `feat`, `fix`, `docs`, `refactor`, `test`, `ci` or `chore`, and `!` after the type for a breaking change. Pull requests are squash-merged, so the title is the commit on main: the `pr-title` check (`.github/workflows/pr-title.yml`) fails a title that is not a Conventional Commit, and release-please (`.github/workflows/release-please.yml`) reads the commits on main to open the release pull request with the next version and the changelog.

## Skills

Agent skills are in `.agents/skills/<name>/SKILL.md`; `.claude/skills` links to that folder.
