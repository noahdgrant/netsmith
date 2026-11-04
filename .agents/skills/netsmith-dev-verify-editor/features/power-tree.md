# Power tree

## How to get to it

The section "Power tree" (key 2). For a board of a system it shows the whole system: a card for each net a rail draws from, joined left to right by the stages that convert one rail to another and, dashed, by the joins between boards.

## Driving it

`scripts/editor-verify drive power` (fixture `two_boards`), `drive single-board` (launch with `--fixture regulator`)

1. Cards `VIN`, `3V3` (power board) and `3V3` (led board), each with its voltage range, "Supplied by" and "Loads" lines, and a budget bar.
2. The power board's `3V3` card reads `0A..50mA of 100mA` and "within what the supply can give (stage.regulator (LinearRegulator))": the regulator's `max_current`, counted once.
3. `VIN` says "no supply limit is stated": it is supplied from outside.
4. Click a load line such as `stage.vin`: the editor goes to the rail's declaration.

### A big board: `drive big-power` after `launch --fixture big`

Rails are cards in columns left to right by supply (`VIN` first, then what it feeds), 320 px wide with names wrapped and the card as tall as its content. A column with many cards that nothing feeds is cut into lanes so the whole is not a strip. Each converter (module or part with an input rail and an output rail) is an edge from every rail it takes to every rail it makes. Loads are listed heaviest first (by the largest current they can draw). Numbers are the library's: four significant digits with a prefix (`137mA`, `3.584V..3.766V`), residue as `0A`. A card whose demand tops its capacity has a red border and an OVER badge; the recipe changes the answer in flight to show it because the solver would not build such a board.

## Gotchas

- Every number comes from the solved model (`Model`, `Balance`), not from text. A net whose current the solver has not settled says so and has no bar.
- A card turns red only if the demand's top exceeds the capacity. A board that fails the solver's own check does not build, so you see it only on a stale drawing; the unit tests in `crates/netsmith/src/editor/power.rs` cover it.
- The graph is fitted to the section, which has the whole window unless the window is split.
