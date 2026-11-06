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

Then run the fab gate. It runs netsmith's own design rules check (no KiCad needed) and must end with `design rules check: passed`. Add `--cross-check` (needs KiCad, below) to compare the files and the findings with KiCad's.

```sh
cargo run -p netsmith -- fab examples/linear_regulator.nsm --out /tmp/fab-<branch>
```

The examples in `examples/*.nsm` must still build, except `examples/carrier_and_cards.nsm`, whose invented parts have no KiCad symbol: it is checked and solved (`cargo run -p netsmith-solve --example solve -- examples/carrier_and_cards.nsm`) and its tests are in `crates/netsmith-check/tests/mate.rs` and `crates/netsmith-solve/tests/mate.rs`.

## The editor

For a change to the editor or to a view it shows, also run the browser check in `netsmith-dev-verify-editor` (`scripts/editor-verify`) and look at the screenshots.

## The editor's committed page

`crates/netsmith-editor/web-dist` is the built page, committed so netsmith builds without Node. `SOURCE-HASH` in it is a hash of the bytes of the sources in `web/`, and `build.rs` embeds it only when that is the sources' hash (never by file times). After any change under `web/`, with Node 20+, run `scripts/web-bundle update` and commit `web-dist`. `scripts/web-bundle check` rebuilds the page and fails, naming the files, if `web-dist` differs; `cargo test -p netsmith-editor --test bundle` fails without Node when a source changed and `update` was not run. To see a no-Node build, put a `PATH` and `HOME` without Node on a fresh `git clone` (`build_web.rs` also looks in `~/.nvm`): `cargo build -p netsmith` must print no warning and `netsmith editor` must serve the page.

## Language guide

`cargo test -p netsmith --test guide` runs every `nsm` block of `docs/LANGUAGE.md`, compares every message block with what the tool prints, and checks the skills. After you change a message or a std definition, run it with `GUIDE_BLESS=1` to rewrite the message blocks, then read the diff. List a new std definition in the guide's catalogue.

## KiCad

`netsmith fab`, the editor and the tests find `kicad-cli` in this order: `$KICAD_CLI`, `kicad-cli` on `PATH`, then under WSL the newest Windows KiCad in `/mnt/c/Program Files/KiCad/<version>/bin/kicad-cli.exe` (ADR 0021). The shell scripts (`scripts/check-kicad.sh`) still default to the 9.0 path. Tests that need KiCad skip themselves when it is absent. Say so in your report when they skipped.

`build` and `fab` find KiCad's `symbols` and `footprints` folders with `netsmith_kicad::install` (ADR 0021): `NETSMITH_KICAD_SYMBOLS` and `NETSMITH_KICAD_FOOTPRINTS`, then beside `kicad-cli`, then the usual install places. A test of the lookup builds a fake install tree with `install::Search::under(<temp dir>)`; do not rely on this machine's KiCad.

The Gerbers, drill files and placement file are netsmith's own (ADR 0034). After changing `crates/netsmith-fab` (`plot`, `fill`, `pads`, `drill`, `font`), run `cargo run -p netsmith-fab --example crosscheck -- <board.kicad_pcb> --out /tmp/xc` on a board with the feature you changed: it plots with KiCad too and writes a picture of each layer that differs (grey both, white only netsmith, dark grey only KiCad). `crates/netsmith-fab/tests/data/features.kicad_pcb` has every shape; extend it (and its generator's idea, a plain text file) when you add a feature.

Check a generated schematic and board against real KiCad (needs `poppler-utils`):

```sh
scripts/check-kicad.sh out/LedExample/LedExample.kicad_sch
crates/netsmith-pcb/scripts/check-kicad.sh fab/LedExample/LedExample.kicad_pcb
```

Read the last lines. ERC must print `ERC: 0 violation(s)`. DRC must print `DRC: 0 violation(s), 0 unconnected item(s)`. Results land in `out/check/<name>/`. A four-layer board has its zones filled by KiCad's Python first, because KiCad's DRC does not fill zones.

To see how good a placement is, `cargo run --release -p netsmith --example place_report -- <file.nsm> --out <dir> [--seed n]` (from the project's folder for a project) prints the wire length, the ratsnest crossings, the decoupling distances, the routing result and the times, and writes the board for the script above. `NETSMITH_WRITE_BOARDS=<dir>` on the `measured` test of `crates/netsmith-pcb/tests/synthetic.rs` and `NETSMITH_KEEP_BOARDS=<dir>` on the tests of `kicad_drc.rs` keep the boards they check; `NETSMITH_TEST_SEED=<n>` on those tests asks the placer for another arrangement of every board (ADR 0033).

### Several KiCads at once

Every start of a Windows program under WSL (`kicad-cli.exe`, KiCad's `python.exe`, `cmd.exe`) goes through `netsmith_fab::exclusive::run`. Reason: with many starts at once (the tests of one `cargo test` run, plus the runs of other agents) WSL's interop channel sometimes fails a start before the program runs. The only output is `<3>WSL (...) ERROR: UtilAcceptVsock:273: accept4 failed 110`, and a test such as `the_led_example_agrees_with_kicads_own_plot` fails, then passes when run alone. `run` takes a lock (a mutex, then `flock` on `netsmith-kicad.lock` in the temp directory) so one KiCad program runs at a time on the machine, starts a program again (three attempts in all) when the failure is WSL's or a signal's and not the program's, and puts KiCad's stdout, stderr, exit status and every earlier attempt in the failure message. A launch that waits more than 30 minutes for the lock says so and names the file. Scratch directories are `netsmith-fab-<pid>-<n>-<time>`.

New code and tests that start a Windows program use `netsmith_fab::exclusive::run`, not `Command` directly.

After changing that code, or when a KiCad test fails only in a full run, run `scripts/stress-kicad.sh [runs] [processes] [test names]` (default: 50 runs of the three cross-check tests of `crates/netsmith/tests/fab.rs`, 3 processes at once, 4 test threads each). It must report 0 failed; a failure prints the log with the full message. It takes about 25 minutes.

To see how good the schematic view's drawing is, `cargo run -p netsmith --example schematic_metrics -- <file.nsm | project folder> [--plain] [--violations]` prints one line per sheet (bends, crossings, wire length, satellite distances, overlaps, shape) and the rules the drawing breaks (a short, an open, a wire through a symbol, a wire end that touches nothing, a double line, a hook, a missing or an unused dot, a wire hugging a symbol, two texts over each other); the numbers and their definitions are in `crates/netsmith-view/src/metrics.rs`, the baseline in `docs/research/schematic-metrics-baseline.md`. A big project takes most of a minute to analyse and the drawing a second: `--save-inputs <file>` writes what the drawing is made from, `--inputs <file>` draws from that file alone, and `--svg <file>` writes the picture (`<file>-<board>.svg` for each board of a file of several), so a change to the layout can be judged by looking at once: cut it into tiles of about 120 x 72 mm and look at every one, as the machine rules do not find all that a person sees (`NETSMITH_DUMP_SVG=<folder> cargo test -p netsmith-view --test invariants dump_the_corpus_when_asked` writes the pictures of the test boards) (copy a private project to `/tmp`; never put its files or its inputs file in the repository).

A clean DRC means the board meets the process's published limits. It does not mean JLCPCB's own check accepts it.

## When a gate fails

- Fix the cause. Do not weaken a test or silence a lint.
- A new behaviour needs a test, including its failure messages. Messages must name the numbers and fields involved.
- Done means every gate passes and you can say what you did not verify.
