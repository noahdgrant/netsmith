# netsmith

A language and tools for designing electronics as code. `README.md` says what it is and how to run it.

## Build, test, lint

- Test with `cargo test --workspace`, lint with `cargo clippy --workspace --all-targets`, and check formatting with `cargo fmt --all --check`. Each passes with no warnings. CI runs fmt, clippy and the tests on every pull request.
- The commands are in `netsmith --help`; `cargo run -p netsmith -- build examples/led.nsm --out out` builds a sample board.
- The editor's page is committed as a built bundle in `crates/netsmith-editor/web-dist`. Only a change to its sources in `crates/netsmith-editor/web` needs Node 20 or newer: run `scripts/web-bundle update` and commit the bundle.

## Tests

- A test that needs a folder takes it from `netsmith_kicad::testlib::TempDir` (or `TempFile`), which removes it when the test ends, also on a panic. Bind the guard to a name, never to `_`. `NETSMITH_KEEP_TEST_DIRS=1` keeps the folders of a failing run.
- `docs/LANGUAGE.md` is the language guide, and `crates/netsmith/tests/guide.rs` runs its examples. Change the guide with the language.

## Data

A part number goes into a catalog only with a recorded source (`data/jlcpcb/README.md`).

## Vocabulary and decisions

- Use the terms in `GLOSSARY.md` and avoid the ones listed under _Avoid_.
- Decisions that are hard to reverse are in `docs/adr/`; the language's design rules are in `docs/DESIGN-RULES.md`.
- Language-design choices (syntax, semantics) are the owner's. Present options and ask.
- Open work is in GitHub issues.

## Git

Stage files by explicit path when committing, so other people's in-progress files stay out.

## Skills

Agent skills are in `.agents/skills/<name>/SKILL.md`; `.claude/skills` links to that folder.
