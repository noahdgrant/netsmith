---
name: netsmith-route-a-board
description: Finish a board the router left open, fix a copper area, or keep copper a person gave, with `netsmith routes`, `board fab --reuse-routes` and `board drc`. Use when `fab` says connections could not be routed, `routing-failure.txt` or `routing-report.json` exists, a user draws or keeps tracks, vias or zones, or the design rules check names copper.
---

# Route a board

The router draws copper for every connection the routes file does not make. The **routes file**, `<stem>.routes.toml` beside the source, holds the copper a person, an agent or an import decided: tracks, vias and zones of nets, each shape optionally locked. Only `netsmith routes` writes it, and it is committed with the source. The router's own copper is a cache (`.netsmith/routes/`, never committed): `fab` never writes it into the routes file. Where the part stands is the layout file (`netsmith-use-layout`); the format, the refusals and the rip-up rounds are in the guide's "The routes file" and "Connections left: rounds of rip-up" (`docs/language/23-the-shape-of-a-board.md`). `netsmith routes --help` and each subcommand's help have the flags.

## Loop

1. **Route and read what is left**: `netsmith board fab`. A board the router could not finish still gets its files in `fab/`, marked `INCOMPLETE`, and the run ends in the error `N connections could not be routed`; never order from it. Read `fab/<board>/routing-failure.txt` (each connection left with its net and two pads, the stage of the router it last failed in, and what stands in its way: the nets whose copper a way across would cross, or the pads, vias and board edge that wall an end in) and `routing-report.json` (the same as data, with a crowding map: 1 mm squares, `crowding.counts` row by row from `crowding.origin`; the highest squares are where parts need more room). `netsmith routes show --format json` lists the routes file's records, numbered shapes, what of it is not used and the connections still to route. It routes nothing.
2. **Decide who owns the copper.** A shape is the person's when they drew it, a drawing or a mechanical fit fixes it, or it is a net the router must not touch (a sense line, a power path). `routes lock --net N` stops the router adding copper to that net (a connection it still lacks is left, and `fab` says so); `--track N`, `--via N` or `--zone N` keep one shape when the net's pads move. Lock nothing the person did not ask for: the router's copper is the router's. There is no command that copies router copper into the file; draw what must be kept.
3. **Read the failure before drawing.** The stage and the thing in the way say which fix is cheap:
   - a pad walled in by its neighbours, or `pad(s) U1.12, ...` named as the wall: move a part or give it room (`netsmith-use-layout`, `layout fit`), or narrow the process's track and space within the factory's limits;
   - other nets' copper in the way and a crowded square: more room round those parts, a layer more, or a plane or pour for a net that crosses everything;
   - `stopped after the route timeout`: raise `board fab --route-timeout SECS` (0 is no limit); the `Board: routing: ...` lines on stderr say what each try ended with;
   - nothing left to give: `--rip-rounds N` sets the rounds of rip-up of whole nets for a run (`rip_rounds` in `netsmith.toml` for the project, 0 turns them off); try more before drawing by hand.
   - a net of a connection that only other pins would shorten (a GPIO function): `netsmith board pins show`, and `board fab --pins auto|locked`; `netsmith-read-errors` has the pin messages.
4. **Draw by hand** what is left, one shape at a time, in the placement's frame (mm from the outline's top left corner, x right, y down, as `layout set`):
   - `netsmith routes add-track --net N --layer top --from X,Y --to X,Y [--mid X,Y for an arc] [--width MM] [--lock]`; `--layer` is `top`, `bottom` or an inner signal layer; the width defaults to the net's own;
   - `routes add-via --net N --at X,Y [--layers top,inner1] [--lock]` (the process's via by default); a net whose current needs several vias side by side (`via_current`) takes them as one group, `--at X,Y` repeated or `--count N --pitch MM` round one `--at`, checked together (a pitch above the via's diameter leaves them apart);
   - `routes add-zone --net N --layer bottom --rect X,Y,W,H` (or `--outline X,Y` per corner, or `--board-outline`; `--pads`, `--clearance`, `--priority`). A layer that gets its first zone stops drawing the design's pour there; the command says so. Prefer stating a plane or pour in the source (`netsmith-write-nsm`) over a zone, unless the user wants a hand shape.
   Take pad places from `netsmith layout parts --format json`, not from the picture. An end that touches nothing yet is allowed, since a route is drawn a shape at a time; `fab` finds one left so. A command exits 0 when it wrote, 1 when it **refused** and 2 when it could not run. Read a refusal: it names a short, a crossing or a clearance with the place, and the file is as it was. Fix the shape; never loosen a rule to get it in. `routes delete` removes a net's record or one numbered shape, and refuses a locked one: ask before `routes unlock`.
5. **Check without routing**: `netsmith board drc` (`--format json` has the rule, place, nets and parts of each finding; exit 1 on an error) judges the board as it stands, the placement with the routes file's copper, and every connection the file does not make is an `unconnected_items` error, so run it to check a hand-drawn shape, not to see what the router would do. `routes show` says the connections still to route.
6. **Route again**: `netsmith board fab --reuse-routes` starts from the router's kept copper (the editor writes it; it says how many nets it kept and routed again), so a change to one net does not route the whole board. Plain `fab` routes every net and gives the same board on every machine: use it for the last run, and an order never takes the flag. Add `--save-placed` only to give `netsmith dev route` a board.
7. **Stop and ask** when the fix is a change of the board, not of copper: more layers, a larger outline, a part moved off a place the user fixed, a requirement or pin the user states, a rule loosened (`--allow-drc-violations` is never yours to use for an order), or when two attempts at the same connection fail the same way.

## Look

There is no picture of the copper by itself yet (issue #1086). `netsmith view 3d <file.nsm> --view top --picture board.png`, with `--section AXIS=MM` to cut it, shows the board, and `netsmith board copper` sums up the planes, pours and zones by layer. Quote numbers from `routing-failure.txt`, `routing-report.json`, `routes show` and `board drc`, not from a picture.

## Not for this skill

`netsmith dev route` routes a saved placed board with the router's tuning flags (`--tries`, `--stages`, `--arcs`): it is the tool for working on the router, never for finishing a board. `netsmith routes import BOARD.kicad_pcb` takes copper from a KiCad board (every net locked, `--unlocked` to keep none); the port skill (`netsmith-port-a-kicad-board`) says when.

## Report

Tell the user which connections were left and why (the stage and what was in the way), what you changed (a part moved, a `--rip-rounds` or timeout, each shape drawn with its net, layer and place, each lock and why), what `board drc` and the last plain `fab` say, and whatever is still open. A board `fab` marks `INCOMPLETE` is not ready to order, and `netsmith-prepare-fab-order` is the next step only when it is not.
