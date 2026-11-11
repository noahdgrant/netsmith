# What `build` costs on a board with two buck stages

Evidence for the owner's question on a solved-design file (epic #308, item LOCK6, decision of
2026-11-10): how long do `build` and the editor's re-solve take on a regulator-heavy board of about
sixty parts, and which stage dominates? This is a measurement, not a decision. It is not an ADR.

The board is `examples/two_buck/`: a 12 V input, two buck stages (5 V and 3.3 V), a linear
post-regulator, a reverse-polarity diode and a TVS diode, and the parts around them, 59 parts in
the bill of materials (its README says what is invented: the buck IC, `BenchBuck`, because no
standard IC row can be solved, and what the standard parts could not give).

## How it was measured

- Machine: 11th Gen Intel Core i9-11900H at 2.50 GHz, 10 logical CPUs (WSL2), 15 GB of RAM. The
  machine was **shared**: other builds and test runs ran during the measurements (load average 3
  to 6), so absolute numbers are noisy; each is the median of 5 runs, the runs of one board are
  back to back, and the spread of the 5 runs is in the median's neighbourhood except where the load changed
  (the `debug` run of `two_buck` had `solve.pick` from 46 to 80 ms).
- Date: 2026-11-10. Base: `origin/main` at `287f284` plus this change.
- Command: `NETSMITH_NO_KICAD=1 NETSMITH_TIMINGS=1 netsmith build --out <folder>` in the
  project's folder (the stage lines are printed to stderr; `docs/PIPELINE.md` names the stages).
- Profiles: `debug` is the `dev` profile of `Cargo.toml` (with `netsmith-solve`, `-check`, `-core`,
  `-quantity` and `-view` at `opt-level = 2`, so not a plain unoptimised build); `release` is
  `cargo build --release -p netsmith`.
- `libraries` is small here only because KiCad is hidden (`NETSMITH_NO_KICAD=1`): with a KiCad
  install `build` reads its libraries, about 0.3 s (`docs/research/solved-design-and-lock.md`,
  section 4).

## `two_buck`, median per stage

| Stage | `debug` ms | `release` ms |
|---|---:|---:|
| `catalogs` | 4.5 | 2.7 |
| `libraries` (no KiCad) | 2.7 | 0.6 |
| `check` | 15.0 | 5.2 |
| `solve.pick` | 62.0 | 43.1 |
| `solve.dc` | 0.4 | 0.2 |
| `solve` (all) | 66.0 | 45.9 |
| `design` | 3.8 | 0.7 |
| `power` | 0.7 | 0.2 |
| `export` | 23.2 | 5.1 |
| `build.total` | 154.7 | 74.4 |

(`build.total` also holds reading the files, the pick lock and writing the output: 14 ms in
`release`.) On a quieter machine the `debug` run before these gave `solve.pick` 48.4 ms and
`build.total` 121.6 ms.

## The other boards, same command

| Board | BOM lines | `build.total` ms, `debug` / `release` | `solve.pick` ms, `debug` / `release` | `solve.dc` ms, `debug` / `release` |
|---|---:|---:|---:|---:|
| `led` (one file) | 3 | 30.5 / 13.4 | 4.0 / 2.6 | not recorded |
| `linear_regulator` (one file) | 5 | 41.4 / 19.7 | 7.1 / 6.4 | not recorded |
| `enclosure_sensor` | 43 | 84.3 / 46.4 | 16.8 / 14.6 | 0.1 / 0.1 |
| `easyduino_esp32s3` | 35 | 112.6 / 60.9 | 28.6 / 25.2 | 0.2 / 0.1 |
| `orbit` | 48 | 125.3 / 76.8 | 43.2 / 39.5 | 0.1 / 0.1 |
| `rp2350_minimal` | 43 | 128.8 / 77.2 | 41.5 / 38.1 | 0.2 / 0.2 |
| `schematic_bench` | 46 | 131.3 / 76.5 | 50.0 / 40.1 | 0.2 / 0.2 |
| **`two_buck`** | **59** | **154.7 / 74.4** | **62.0 / 43.1** | **0.4 / 0.2** |

## The editor's re-solve

The editor reports `elapsedMs` for each analysis (`GET /api/analysis`: it checks, solves and
designs every board of the project and draws the schematic, so it is more than `build`). Five
edits of the source (a comment appended each time), each read after the change:

| Project | first analysis ms | re-solves ms (5 edits) |
|---|---:|---|
| `two_buck`, `release` | 176 | 166, 165, 185, 149, 140 |
| `two_buck`, `debug` | 234 | 185, 196, 198, 240, 181 |
| `rp2350_minimal`, `release` | 674 | 634, 634, 657, 632, 632 |
| `rp2350_minimal`, `debug` | 896 | 864, 849, 871, 885, 894 |

The editor takes no more time over `two_buck` than over a smaller board with no regulator. The
time per stage inside the editor's analysis was not measured: `NETSMITH_TIMINGS` is read by the
command line only.

## The ERC test

`cargo test -p netsmith --test main erc::the_two_buck_example_passes_erc` builds the example and
runs KiCad's ERC on its schematic; it passes with no violation. KiCad 9 for Windows is installed
on this machine (found through WSL), so the ERC ran: the test finished in 2.4 s and 5.9 s on two
runs, nearly all of it `kicad-cli` starting. For comparison `erc::every_example_passes_erc_with_kicads_own_symbols`
(five one-file boards) took 19 s. The run without KiCad was not measured: the test skips itself
where `kicad-cli` is not found, so it has no such time; `build` without KiCad is the table above.

## The one slow case: a search that fails

The first draft of the board asked each output for `+/- 3 %` from an IC whose reference is
`+/- 1.5 %`; two 1 % resistors cannot hold that, and the solver searched 250000 combinations of
each divider's two resistors before it gave up: `solve.pick` took 10.3 s in `release` (13.5 s in
`debug`), and 21.0 s (25.9 s) when the input rail also had no stated voltage. The same board with
the window the resistors can meet (`+/- 5 %`) picks in 43 ms. The failing search does not depend
on the profile much (a factor of 1.3), and it is the only run in these measurements over 1 s.

## What it means for a solved-design file

On a board that solves, nothing is slow: a 59-part board with two buck stages and a linear
regulator builds in 74 ms in `release` (155 ms in `debug`), `solve.pick` is the largest stage at
43 ms (58 % of `build`; `check` and `export` are 5 ms each and the rest, reading and writing, 14 ms), and the DC analysis
is 0.2 ms, because the islands behind each regulator are small. No stage is near the 1 s that
would justify caching, `solve.pick` is the same 40 ms on `orbit`, `rp2350_minimal` and
`schematic_bench`, and a solved-design file could save at most those 46 ms of 74 ms, and nothing in
the editor, where `two_buck` takes 140 to 185 ms for reasons that are not the solve. The slow
case is a design the solver cannot solve, which a cache of what the solver found cannot hold, so a
solved-design file would not have helped there. One caution on what was not shown: this board
does not reproduce the real 12 V automotive board of ADR 0076 and `Cargo.toml` (about 60 parts,
7.4 s in a plain debug build, 1.25 s with the solver crates optimised); that board is not in the
repository, and its DC and power stages are probably larger than this one's. Recommendation: do
not build a solved-design file on this evidence; keep the pick lock; and, if the owner wants a
cost to work on, it is the budget of a failing pick search (10 to 20 s before a divider that
cannot be met is refused), which is a separate issue. The owner decides.
