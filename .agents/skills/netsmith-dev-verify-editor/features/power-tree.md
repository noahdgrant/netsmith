# Power tree

## How to get to it

The section "Power tree" (key 2). For a board of a system it shows the whole system: every rail a part draws from, and the stages between them, joined left to right; the joins between boards are stages of kind "Board link".

## What it draws

A left-to-right hierarchy (`src/power/layout.ts`, pure and deterministic): sources on the left, each converter, protection stage, filter or switch as a small box (a "stage") between its input rail and its output rail, the output rails with their loads on the right. Edges are right angles with rounded corners; an edge that passes a layer goes through a lane of its own, so none runs through a card, and the stage box carries the label (its kind and what it makes, "Buck 5 V", then its name and type), so no label sits on a line. The kind comes from the stage's type name (Buck, Boost, Ldo/Linear, Protect/Surge, Filter, Switch, Diode), else from what the voltage does.

A rail card (`src/views/Power.tsx`, numbers from `src/power/format.ts`): the name, the nominal voltage and its range, what supplies it (a line with a leading arrow and negative numbers), the total load as `min .. max`, a bar and a line "Headroom 11 mA of 100 mA" (or "Over by ...", or "No supply limit stated"), and one summary line "28 loads". The summary opens the list: the five heaviest loads, "+N more", loads that settled at 0 A in ONE line "22 loads at 0 A" that opens on a click, and loads with an unsettled current. Currents are in A, mA or uA with three significant digits and tabular figures. The converters a rail feeds are not listed as loads ("feeds 3 converters" says it). "List all loads" opens every card.

Click a card or stage: its path from the source is lit and the rest dimmed; Escape clears. Arrow keys (with the tree focused, or after a click) move along the tree: Left to what feeds it, Right to what it feeds (back the way it came), Up and Down to the neighbours in the column; Enter opens or closes the loads of the selected rail. Hover a load (or focus it): its path, the part, the pin, the current and "Show in source", which goes to where the rail is declared; clicking the row does the same, and Back returns. The stage name is a link to its source. The minimap (bottom left) shows the whole tree and the window; click or drag in it to move. The zoom bar has Fit and 1:1 (one pixel to one pixel: the text is 12 px or more there; "Fit" shrinks everything to show the whole tree).

The Issues strip lists, worst first, a rail whose total can top its supply's limit (red), a rail with no source, a rail or stage with a current or efficiency the solver has not settled (amber); each is a button that selects the card, and the card has a badge. The editor's own model gives all of it (`/api/power`: nodes, edges and `stages`, with the efficiency of each converter); the checker is not involved.

## Driving it

`scripts/editor-verify drive power` (fixture `two_boards`), `drive single-board` (launch with `--fixture regulator`), `drive big-power` (`--fixture big`), and, for the invented board in `scripts/editor-verify-js/fixtures/power_tree.nsm` (8 rails, a buck, a boost, a linear regulator, a protection stage, a filter, a load switch, a backup diode, 22 reference pins at 0 A on one rail), `launch --fixture power_tree` then `drive power-tree power-issues`.

`power-tree` asserts: 8 rail cards and 7 stages; every edge runs left to right; no sampled point of any edge lies inside a card it does not join; no two cards overlap and every name lies inside its card; no text under 12 px; the card of the 3.3 V rail (range, total, headroom 11 mA of 100 mA, "28 loads"); the list (five heaviest, "+1 more", "22 loads at 0 A" opens to 22 rows and folds again); a click lights the chain VIN, protection, VPROT, buck, V5, LDO, V3V3 (6 edges, 8 dimmed nodes); the arrow keys; hover (part, pin, current) and "Show in source" (one pane stays one pane, Back returns); a stage name goes to its source. `power-issues` changes the answer in flight (the solver will not build an overdrawn board) to show a red card, "No source", an unsettled current and an unsettled efficiency, and an issue button that jumps to its card.

## Gotchas

- Every number comes from the solved model (`Model`, `Balance`), not from text. A supply is a current range that ends at zero or below; the solver's end of a range that stops at zero can be a hair above it (`5e-324`), which `crates/netsmith/src/editor/power.rs` treats as zero (`NEGLIGIBLE`), or the supply would count as a load and its converter would have no edge.
- A rail turns red only if the demand's top exceeds the capacity. A board that fails the solver's own check does not build, so you see it only on a stale drawing; the unit tests in `power.rs` cover it.
- `/api/power` has no field for what a rail is for; the stage kind is read from its type name, and a type named otherwise is a plain "Converter" unless the voltage says buck or boost.
- The graph is fitted to the section, which has the whole window unless the window is split. The whole tree is wide: at 1366 px it fits at about 60 %, so read it at 1:1 or zoom.
- Fixture stages are `part`s with no pads, so `power_tree.nsm` solves and shows in the editor but cannot be written to KiCad.
