---
name: netsmith-dev-verify
description: Run netsmith's quality gates and the real-KiCad checks (fmt, clippy, tests, the fab design rules check, ERC and DRC scripts). Use when finishing a change to the netsmith repository, when a check fails, or when several agents build at once.
---

# Verify a change to netsmith

Entry point: `AGENTS.md`. Run these from the repository root.

## Gates

Run all three. Each must pass with no warnings.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

A test that fails once and passes on a rerun is a bug to fix, not to retry: no test passes or fails by the speed of the machine (`AGENTS.md`, Tests). A cancel test counts the checkpoints a call reached after the cancel (`Cancel::checkpoints`, `Cancel::at_checkpoint`), a scaling test compares counts of work between two sizes, and a speed worth seeing is printed by an `#[ignore]`d `measured` test (`cargo test -p netsmith-router --test main mixed::measured -- --ignored --nocapture`). A time limit in a test is a hang guard of a minute or more.

Then run the fab gate. It runs netsmith's own design rules check (no KiCad needed) and must end with `design rules check: passed`. Add `--cross-check` (needs KiCad, below) to compare the files and the findings with KiCad's.

```sh
cargo run -p netsmith-cli -- fab examples/linear_regulator.nsm --out /tmp/fab-<branch>
```

The examples in `examples/*.nsm` must still build, except `examples/carrier_and_cards.nsm`, whose invented parts have no KiCad symbol: it is checked and solved (`cargo run -p netsmith-solve --example solve -- examples/carrier_and_cards.nsm`) and its tests are in `crates/netsmith-check/tests/mate.rs` and `crates/netsmith-solve/tests/mate.rs`.

## The examples' committed files

Each example commits its pick lock (`<stem>.lock.toml`) and its design (`<stem>.design.json`, what the design stage hands the placer) beside its source. A change to the checker, the solver, the design stage, `std/` or a catalog can move them. Check both after such a change:

```sh
scripts/check-example-locks.sh
scripts/check-example-designs.sh
```

To fix a stale lock, run `netsmith lock --update` in the example. To fix a stale design file, run `scripts/check-example-designs.sh --update`. Read the diff of each file before you commit it: a moved pick or design is a change to the board.

## The editor

For a change to the editor or to a view it shows, also run the browser check in `netsmith-dev-verify-editor` (`scripts/editor-verify`) and look at the screenshots.

## The editor's page

No built page is committed (ADR 0114). `crates/netsmith-editor/build.rs` builds the page from `web/` with Node 20+ when Node is there (`PATH`, else `~/.nvm`) and embeds it; without Node the binary embeds a placeholder page and `cargo build` prints a warning (`cargo:warning=the editor's page was not built...`). `scripts/web-bundle build` builds `web/dist` by hand, `scripts/web-bundle hash` prints the hash of the sources. `cargo test -p netsmith-editor --test main bundle::` checks the page the build embedded (complete, of the current sources, the 3D chunk within its budget); with no page in the build it skips with a message, and `NETSMITH_REQUIRE_BUNDLE_CHECK=1` (CI sets it) makes a placeholder fail it. To see a no-Node build, put a `PATH` and `HOME` without Node on the build (`build_web.rs` also looks in `~/.nvm`): `cargo build -p netsmith-cli` must succeed with that warning and `netsmith editor` must serve the placeholder (a 503 page). Never commit `web-dist`.

## Language guide

`cargo test -p netsmith-tools --test main guide::` runs every `nsm` block of the chapters in `docs/language/`, compares every message block with what the tool prints, and checks the skills. After you change a message or a std definition, run it with `GUIDE_BLESS=1` to rewrite the message blocks, then read the diff. List a new std definition in the guide's catalogue.

## KiCad

`netsmith fab`, the editor and the tests find `kicad-cli` in this order: `$KICAD_CLI`, `kicad-cli` on `PATH`, then under WSL the newest Windows KiCad in `/mnt/c/Program Files/KiCad/<version>/bin/kicad-cli.exe` (ADR 0021). The shell scripts (`scripts/check-kicad.sh`) still default to the 9.0 path. Tests that need KiCad skip themselves when it is absent. Say so in your report when they skipped.

`build` and `fab` find KiCad's `symbols` and `footprints` folders with `netsmith_kicad::Search` (ADR 0021): `NETSMITH_KICAD_SYMBOLS` and `NETSMITH_KICAD_FOOTPRINTS`, then beside `kicad-cli`, then the usual install places. A test of the lookup builds a fake install tree with `Search::under(<temp dir>)`; do not rely on this machine's KiCad.

No run needs KiCad (ADR 0097): `NETSMITH_NO_KICAD=1` makes the search find nothing, and `scripts/check-no-kicad.sh [path/to/netsmith]` builds and fabs every example with the libraries and `kicad-cli` hidden (the `no-kicad` CI job runs it). Run it after a change to the exporters, `std/kicad.toml` or the libraries. A `Libraries` of a test that must not see this machine's KiCad is `Libraries::for_project_by(root, &Search::under(<temp dir>).with_no_kicad())`.

The Gerbers, drill files and placement file are netsmith's own (ADR 0034). After changing `crates/netsmith-fab` (`plot`, `fill`, `pads`, `drill`, `font`), run `cargo run -p netsmith-fab --example crosscheck -- <board.kicad_pcb> --out /tmp/xc` on a board with the feature you changed: it plots with KiCad too and writes a picture of each layer that differs (grey both, white only netsmith, dark grey only KiCad). `crates/netsmith-fab/tests/data/features.kicad_pcb` has every shape; extend it (and its generator's idea, a plain text file) when you add a feature.

Check a generated schematic and board against real KiCad (needs `poppler-utils`):

```sh
scripts/check-kicad.sh out/LedExample/LedExample.kicad_sch
crates/netsmith-pcb/scripts/check-kicad.sh fab/LedExample/LedExample.kicad_pcb
```

Read the last lines. ERC must print `ERC: 0 violation(s)`. DRC must print `DRC: 0 violation(s), 0 unconnected item(s)`. Results land in `out/check/<name>/`. A four-layer board has its zones filled by KiCad's Python first, because KiCad's DRC does not fill zones.

To see how good a placement is, `cargo run --release -p netsmith-tools --example place_report -- <file.nsm> --out <dir> [--seed n]` (from the project's folder for a project) prints the wire length, the ratsnest crossings, the decoupling distances, the routing result and the times, and writes the board for the script above. `NETSMITH_WRITE_BOARDS=<dir>` on the `measured` test of `crates/netsmith-router/tests/synthetic.rs` and `NETSMITH_KEEP_BOARDS=<dir>` on the tests of `crates/netsmith-fabricate/tests/kicad_drc.rs` keep the boards they check; `NETSMITH_TEST_SEED=<n>` on those tests asks the placer for another arrangement of every board (ADR 0033).

### Several KiCads at once

Every start of a Windows program under WSL (`kicad-cli.exe`, KiCad's `python.exe`, `cmd.exe`) goes through `netsmith_fab::run_exclusive`. Reason: with many starts at once (the tests of one `cargo test` run, plus the runs of other agents) WSL's interop channel sometimes fails a start before the program runs. The only output is `<3>WSL (...) ERROR: UtilAcceptVsock:273: accept4 failed 110`, and a test such as `the_led_example_agrees_with_kicads_own_plot` fails, then passes when run alone. `run` takes a lock (a mutex, then `flock` on `netsmith-kicad.lock` in the temp directory) so one KiCad program runs at a time on the machine, starts a program again (three attempts in all) when the failure is WSL's or a signal's and not the program's, and puts KiCad's stdout, stderr, exit status and every earlier attempt in the failure message. A launch that waits more than 30 minutes for the lock says so and names the file. Scratch directories are `netsmith-fab-<pid>-<n>-<time>`.

New code and tests that start a Windows program use `netsmith_fab::run_exclusive`, not `Command` directly.

After changing that code, or when a KiCad test fails only in a full run, run `scripts/stress-kicad.sh [runs] [processes] [test names]` (default: 50 runs of the three cross-check tests of `crates/netsmith-fabricate/tests/fab.rs`, 3 processes at once, 4 test threads each). It must report 0 failed; a failure prints the log with the full message. It takes about 25 minutes.

To review the look of the symbols netsmith draws itself (ADR 0097) beside KiCad's, run `cargo run -p netsmith-tools --example symbol_gallery -- --out docs --file symbol-gallery.html`: it writes one self-contained page, the committed `docs/symbol-gallery.html`, with every symbol of `crates/netsmith-kicad/src/passives.rs` and four ICs drawn from pin tables; KiCad's stock symbols are drawn where its library is installed (`--no-kicad` leaves them out). Regenerate and commit it when a symbol's look changes; a test (`cargo test -p netsmith-tools --test main symbol_gallery::`) fails when a symbol is not in it.

To see how good the schematic view's drawing is, `cargo run -p netsmith-tools --example schematic_metrics -- <file.nsm | project folder> [--plain] [--violations]` prints one line per sheet (bends, crossings, wire length, satellite distances, overlaps, shape) and the rules the drawing breaks (a short, an open, a wire through a symbol, a wire end that touches nothing, a double line, a hook, a missing or an unused dot, a wire hugging a symbol, two texts over each other); the numbers and their definitions are in `crates/netsmith-view/src/metrics.rs`, the baseline in `docs/research/schematic-metrics-baseline.md`. A big project takes most of a minute to analyse and the drawing a second: `--save-inputs <file>` writes what the drawing is made from, `--inputs <file>` draws from that file alone, and `--svg <file>` writes the picture (`<file>-<board>.svg` for each board of a file of several), so a change to the layout can be judged by looking at once: cut it into tiles of about 120 x 72 mm and look at every one, as the machine rules do not find all that a person sees (`NETSMITH_DUMP_SVG=<folder> cargo test -p netsmith-view --test main invariants::dump_the_corpus_when_asked` writes the pictures of the test boards) (copy a private project to `/tmp`; never put its files or its inputs file in the repository).

The drawing depends on the design alone: no environment variable changes it (there are no switches to draw without a part of the layout, to compare). To measure what a change to the layout does, build the commit before it (a separate `CARGO_TARGET_DIR`), run the same commands on both, and compare the `--plain` output and the pictures with `diff -r`: a refactor must leave them byte-identical.

A clean DRC means the board meets the process's published limits. It does not mean JLCPCB's own check accepts it.

## When a gate fails

- Fix the cause. Do not weaken a test or silence a lint.
- A new behaviour needs a test, including its failure messages. Messages must name the numbers and fields involved.
- Done means every gate passes and you can say what you did not verify.
