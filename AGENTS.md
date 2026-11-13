# netsmith

A language and tools for designing electronics as code. `README.md` says what it is and how to run it.

## Build, test, lint

- Test with `cargo test --workspace`, lint with `cargo clippy --workspace --all-targets`, and check formatting with `cargo fmt --all --check`. Each passes with no warnings. CI runs fmt, clippy and the tests on every pull request, and the editor's browser recipes (`scripts/editor-verify-all`) in a `recipes` job. The `test`, `kicad` (the tests that can ask for KiCad, with KiCad 9 installed, after `test` passes) and `recipes` jobs are slices that run side by side (4, 3 and 2); `scripts/ci-kicad-tests` says which tests `kicad` runs and fails `test` when a test looked for KiCad outside them, and `scripts/ci-test-slices` and `scripts/ci-recipe-slices` prove that every test and every recipe runs in exactly one slice, and `scripts/ci-recipe-slices` says how to rebalance the recipes. One `build` job compiles the workspace (on a pull request, only the packages its change can reach: `scripts/ci-rdeps`, which `clippy` also follows; `scripts/ci-rdeps-test` pins it) and hands the archive of the tests (`cargo nextest archive`) and the `netsmith` binary to the other jobs, which compile nothing. A pull request runs only the jobs its change needs (`scripts/ci-changes` has the rules and `scripts/ci-changes-test` pins them, in the `changes` job; a push to main runs everything), and `ci-ok` is the one check to require. The `no-kicad` job (a runner with no KiCad) runs `scripts/check-no-kicad.sh`, which builds and fabs every example with KiCad hidden: `build`, `fab` and the editor need no KiCad install (ADR 0097). KiCad 9 and Chromium's system libraries are installed from a cache of .deb files (`.github/actions/apt-cache`, `scripts/ci-apt-cache`, filled daily by `.github/workflows/apt-cache.yml`), so a job passes when Ubuntu's mirrors are down. The `locks` job runs `scripts/check-example-locks.sh`, which runs `netsmith lock --check` over every example (with no KiCad) and fails when a committed pick lock (`<stem>.lock.toml`) is stale or missing: a change to a catalog, std or the solver that moves a pick of an example needs `netsmith lock --update` in the example and a reviewed diff of the lock (ADR 0100). The `designs` job runs `scripts/check-example-designs.sh`, which builds every example with `netsmith build --emit-design` and fails when a committed design file (`<stem>.design.json`, what the design stage hands the placer) is stale or missing: a change that moves a design needs `scripts/check-example-designs.sh --update` and a reviewed diff of the design files.
- `rust-toolchain.toml` pins the Rust version, here and in CI, so local clippy is CI's clippy. A new Rust release changes nothing until a pull request raises the pin and fixes what the new clippy finds.
- A test build is heavy: in a crate with no `tests/main.rs` every file of its `tests/` folder is its own executable (#190: every crate with a `tests/` folder has one executable, `tests/main.rs`, with the files as modules, found by `automod::dir!("tests")`: a new file is a module with no list to edit, a folder of helpers (`common/`) still needs its own `mod` line, and one file's tests run with `cargo test -p netsmith --test main guide::`). The dev profile keeps line tables only (see `Cargo.toml`; `CARGO_PROFILE_DEV_DEBUG=2` brings the full debug information back for a debugger, `=0` drops it for the smallest build). Where memory is short, or several builds run at once, limit the linkers with `CARGO_BUILD_JOBS=3` and take turns with `flock /tmp/netsmith-heavy.lock cargo test ...` (one heavy build at a time; a machine ran out of memory with three).
- The commands are in `netsmith --help`; `cargo run -p netsmith-cli -- build examples/led.nsm --out out` builds a sample board.
- The command line is the `netsmith-cli` crate (its binary is `netsmith`). A command is one file, `crates/netsmith-cli/src/commands/<name>.rs`, with its clap `Args` (the doc comments are its help) and its `run`, plus a variant and a match arm in `commands/mod.rs`. A test that starts the binary goes in `crates/netsmith-cli/tests`; a test of the library stays in `crates/netsmith/tests`.
- The editor's page is not committed (ADR 0114): `build.rs` builds it from `crates/netsmith-editor/web` with Node 20 or newer when Node is present, and embeds a placeholder page that says it was not built when not (`cargo build` still succeeds, with a warning). Releases and CI always have Node, so they ship and test the real page: `NETSMITH_REQUIRE_BUNDLE_CHECK=1` makes the page tests fail on a placeholder instead of skipping. `scripts/web-bundle build` builds the page by hand into `crates/netsmith-editor/web/dist`; never commit `crates/netsmith-editor/web-dist` (it is ignored).

## Tests

- A test that needs a folder takes it from `netsmith_testkit::TempDir` (or `TempFile`; the crate `netsmith-testkit` is a `[dev-dependencies]` entry only), which removes it when the test ends, also on a panic. Bind the guard to a name, never to `_`. `NETSMITH_KEEP_TEST_DIRS=1` keeps the folders of a failing run.
- A test should take under about 2 minutes on CI (nextest names a test past a minute as SLOW, `.config/nextest.toml`). A board that needs longer becomes several tests, one per seed or case, or a `measured` test that is `#[ignore]`d, as `mixed::measured` is.
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
