## Commands

`netsmith --help` and `netsmith <noun> <verb> --help` document every command and flag.

### The command line

The commands are grouped under nouns, each with its verbs (issue #1090): `netsmith <noun> <verb>`, so `netsmith --help` lists nine nouns and `netsmith board --help` the verbs of a board.

- `project`: the project as a whole: make one (`init`), bring its agent files up to this netsmith (`refresh`), refresh or check its pick lock (`lock`), make one of its orders (`order`).
- `board`: what is done to the boards of a file or of the project: `check`, `build`, `fab`, `explain` a pick, `compare` two designs.
- `layout`: the layout file, where each part is.
- `routes`: the routes file, the copper fixed by hand or imported.
- `schematic`: the schematic layout file (`<stem>.sch.toml`), how the schematic is arranged: `show` it, `set` an entry, `check` it against the board, `arrange` the sheets of a big board (issue #1093). It is `layout`'s counterpart for the drawing: a noun is the thing a file holds, so the PCB's solution is `layout` and the schematic's arrangement is `schematic`, and the picture of it is the verb `view schematic`.
- `view`: look at a board: `3d` draws it to a PNG, `editor` serves the editor, `editor-3d` drives the 3D view of a running editor.
- `catalog`: the rows of the catalogs, checked against LCSC's record.
- `import`: what comes into a project from outside: a DXF drawing (`dxf`), KiCad's library tables (`kicad-libs`), a part from a symbol (`part`), a port of a KiCad board (`port`), a case's STEP model (`case`).
- `dev`: the tools for developing netsmith and checking ports: `route` routes a saved placed board with the router's tuning flags, `parity` compares a port with its original, `board3d` prints the 3D scene's report.

The names follow four rules:

- One name for one thing. A command is named by the noun it acts on and its verb, and one verb is never a second spelling of another. A command or flag of an earlier release is an error that names the one to write and prints the whole line rewritten (ADR 0001; `docs/MIGRATING.md`).
- `--out` is always a folder. A command that writes one file names the flag after the file: `view 3d --picture` and `view editor-3d --picture` (a PNG), `layout export --kicad-pcb`, `dev board3d --glb` and `--step`, and `--write` for the source an import writes.
- One way to choose a board: the source file (`FILE.NSM`; with none, the project's boards) and `--board NAME` for one board of a file with several. Where the command takes another file first (`routes import BOARD.KICAD_PCB`, `layout set PATH`), the source is `--file FILE.NSM`.
- `--format json` wherever there is output for a program to read: one JSON document on stdout, and nothing else there.

Each command, what it was before, and its flags (a test walks `--help` of every command and holds this table to it, `crates/netsmith-cli/tests/cli_table.rs`):

| Command | Before | Flags |
|---|---|---|
| `netsmith project init` | `netsmith init` | `--name` |
| `netsmith project refresh` | `netsmith init --refresh` | none |
| `netsmith project lock` | `netsmith lock` | `--update`, `--check`, `--part`, `--except` |
| `netsmith project order` | `netsmith fab --order [NAME]` | `--out`, `--route-timeout`, `--allow-drc-violations`, `--cross-check`, `--strict`, `--locked`, `--constraints`, `--no-glb`, `--no-step`, `--offline`, `--quiet`, `--place-seeds`, `--place-budget`, `--progress`, `--to-editor` |
| `netsmith board check` | `netsmith check` | `--strict`, `--format`, `--notes` |
| `netsmith board build` | `netsmith build` | `--out`, `--strict`, `--locked`, `--emit-design`, `--emit-solved`, `--format`, `--notes` |
| `netsmith board fab` | `netsmith fab` | `--out`, `--route-timeout`, `--allow-drc-violations`, `--cross-check`, `--strict`, `--locked`, `--constraints`, `--pins`, `--no-glb`, `--no-step`, `--save-placed`, `--reuse-routes`, `--offline`, `--quiet`, `--place-seeds`, `--place-budget`, `--progress`, `--to-editor`, `--format`, `--notes` |
| `netsmith board explain` | `netsmith explain` | `--board`, `--all-rows` |
| `netsmith board compare` | `netsmith compare` | `--board`, `--map`, `--by-refdes`, `--ignore-unconnected`, `--loose-values`, `--format` |
| `netsmith board pins show` | new | `--file`, `--board`, `--format` |
| `netsmith board pins lock` | new | `--file`, `--board`, `--format` |
| `netsmith board pins unlock` | new | `--file`, `--board`, `--format` |
| `netsmith board pins move` | new | `--file`, `--board`, `--format` |
| `netsmith board pins apply` | new | `--proposal`, `--file`, `--board`, `--format` |
| `netsmith layout show` | the same | `--file` (was `--into`), `--board`, `--format` |
| `netsmith layout set` | the same | `--file` (was `--into`), `--board`, `--at`, `--turn`, `--side`, `--lock`, `--unlock`, `--format` |
| `netsmith layout check` | the same | `--file` (was `--into`), `--board`, `--format` |
| `netsmith layout summary` | the same | `--file` (was `--into`), `--board`, `--near`, `--within`, `--between`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith layout compare` | the same | `--file` (was `--into`), `--board`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith layout regenerate` | the same | `--file` (was `--into`), `--board`, `--seed`, `--margin`, `--write`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith layout capture` | the same | `--file` (was `--into`), `--board`, `--dry-run`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith layout fit` | the same | `--file` (was `--into`), `--board`, `--near`, `--step`, `--side`, `--as-laid-out`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith layout parts` | the same | `--file` (was `--into`), `--board`, `--as-laid-out`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith layout export` | the same | `--file` (was `--into`), `--board`, `--kicad-pcb` (was `--out`), `--place-seeds`, `--place-budget`, `--format` |
| `netsmith layout import` | the same | `--file` (was `--into`), `--board`, `--lock`, `--dry-run`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith routes import` | the same | `--file` (was `--into`), `--board`, `--parts`, `--no-places`, `--unlocked` |
| `netsmith routes show` | the same | `--file` (was `--into`), `--board`, `--format` |
| `netsmith routes add-track` | the same | `--file` (was `--into`), `--board`, `--net`, `--layer`, `--width`, `--from`, `--to`, `--mid`, `--lock` |
| `netsmith routes add-via` | the same | `--file` (was `--into`), `--board`, `--net`, `--at`, `--diameter`, `--drill`, `--layers`, `--lock` |
| `netsmith routes add-zone` | the same | `--file` (was `--into`), `--board`, `--net`, `--layer`, `--outline`, `--rect`, `--board-outline`, `--clearance`, `--min-width`, `--thermal-gap`, `--spoke-width`, `--priority`, `--pads`, `--lock` |
| `netsmith routes delete` | the same | `--file` (was `--into`), `--board`, `--net`, `--track`, `--via`, `--zone` |
| `netsmith routes lock` | the same | `--file` (was `--into`), `--board`, `--net`, `--track`, `--via`, `--zone` |
| `netsmith routes unlock` | the same | `--file` (was `--into`), `--board`, `--net`, `--track`, `--via`, `--zone` |
| `netsmith schematic show` | new (#1093) | `--file`, `--board`, `--format` |
| `netsmith schematic set` | new (#1093) | `--file`, `--board`, `--by`, `--pin`, `--rotate`, `--mirror`, `--no-mirror`, `--at`, `--clear`, `--format` |
| `netsmith schematic check` | new (#1093) | `--file`, `--board`, `--format` |
| `netsmith schematic arrange` | new (#1093) | `--file`, `--board`, `--reset`, `--write`, `--format` |
| `netsmith view 3d` | `netsmith render` | `--board`, `--view`, `--picture` (was `--out`), `--size`, `--section`, `--highlight`, `--xray`, `--case`, `--no-caps`, `--theme`, `--ortho`, `--perspective` |
| `netsmith view editor` | `netsmith editor` | `--port`, `--offline` |
| `netsmith view editor-3d` | `netsmith view` | `--port`, `--board`, `--view`, `--section`, `--highlight`, `--select`, `--xray`, `--no-xray`, `--ortho`, `--perspective`, `--case`, `--colour-by`, `--height-preview`, `--layers-apart`, `--no-layers-apart`, `--format` (was `--json`), `--picture` |
| `netsmith catalog refresh` | the same | `--dry-run`, `--offline`, `--only`, `--lcsc`, `--delay`, `--cache-ttl`, `--no-cache` |
| `netsmith catalog add` | the same | `--lcsc`, `--to`, `--offline`, `--no-cache` |
| `netsmith catalog age` | the same | `--days`, `--only` |
| `netsmith import dxf` | `netsmith dxf import` | `--layer`, `--holes-layer`, `--units`, `--board`, `--write` |
| `netsmith import kicad-libs libraries` | `netsmith kicad-libs import` | none |
| `netsmith import kicad-libs models` | `netsmith kicad-libs models` | `--library` |
| `netsmith import part from-symbol` | `netsmith part from-symbol` | none |
| `netsmith import port skeleton` | `netsmith port skeleton` | `--write` (was `--out`) |
| `netsmith import port tables` | `netsmith port tables` | `--url`, `--reference`, `--check` |
| `netsmith import case place` | `netsmith case place` | none |
| `netsmith import case model` | `netsmith case import` | `--rx`, `--ry`, `--rz`, `--x`, `--y`, `--z`, `--at`, `--holes-at`, `--limits`, `--thickness`, `--limit-step`, `--board`, `--write` |
| `netsmith dev route` | `netsmith route` | `--out`, `--route-timeout`, `--margin`, `--turn`, `--tries`, `--stages`, `--plan`, `--classes`, `--arcs` |
| `netsmith dev parity` | `netsmith parity` | `--table`, `--parts`, `--svg`, `--format` |
| `netsmith dev board3d` | `netsmith board3d` | `--board`, `--format` (was `--json`), `--glb`, `--step`, `--place-seeds`, `--place-budget`, `--progress`, `--to-editor` |

The places left for commands to come, named by the same rules: `board bom`, `board power`, `board drc`, `board swap` and `board copper`; `view schematic`, `view pcb` and `view gerber`; `project fmt` and `project lint`; and `mcp`, the server that offers these commands to an agent.

### The editor

`netsmith view editor` serves the editor on `127.0.0.1` and prints the address to open: a project tree, the source with the compiler's and solver's messages beside the lines they are about, and views of the selected board (schematic, power tree, system, bill of materials, PCB) with buttons for `build` and `fab` (ADR 0019). A board of more than 16 parts is drawn in a box for each module instance, and the boxes, and the cards of the overview of all modules, are set out by how the modules connect: modules joined by nets come one after the other, the same way every time. A card of the overview dragged onto another takes that card's place: the editor writes the sheet's place to the schematic layout file below (`at = [x, y]` in the sheet's table, the top-left corner of its box in millimetres), and the sheets never moved are set out by their connections round it (ADR 0139). A rail is drawn with the ground and supply symbols at its pins. Errors and warnings are listed first, and the notes (relations not evaluated, balances left incomplete) in a closed section. It checks the text as you type without writing it, saves with `Ctrl+S`, reads a file again when it changes on disk, and with no file acts on the project like `build`.

In the schematic, a part can be dragged on the grid, nudged with the arrow keys, turned with `r` and mirrored with `m`. The editor writes where it was put to `<stem>.sch.toml` beside the source file (`src/main.nsm` has `src/main.sch.toml`), so the drawing stays as arranged across edits; `netsmith schematic` reads and writes the same file from the command line (`show` lists its entries and the boxes of the sheets, `set stage.output_cap --by 2.54,0 --rotate 90` writes an entry and keeps the file's comments, `set stage --at 101.6,0` places a sheet, `check` lists the entries that name nothing or that the drawing could not honour, `arrange --write` keeps the arrangement of the overview as `at` entries); the file never holds a wire or a net, an entry that no longer matches the design is listed in the problems and ignored, and deleting the file gives the automatic drawing back (ADR 0061). The editor can also compare the project with a git ref, or with a second ref, and say what changed in the circuit (parts, nets, rails, systems, the PCB, the bill of materials, problems) and not which lines (ADR 0059). An analysis, a comparison, a build and `fab` can be cancelled with their Cancel button; a cancelled analysis leaves the last reading (ADR 0062).

The editor's bill of materials marks the lines whose price and stock are old (as `netsmith catalog age` lists them), says `unknown` for the rest, and has a "Check parts against LCSC" button for the project's own catalogs.

### Power findings

The power model's findings come from `netsmith`, not from the editor's page: `build`, the editor's analysis (its `power` stage, the Problems dock's `Power` group) and the power tree it draws all carry the same ones. A rail that can draw more than the supply that states what it can give (`max_current`) is an error that stops the build, where the solver had no relation to check it against; `--strict` is not needed. The notes do not stop it, with or without `--strict`: a rail nothing supplies (`V5 has no source: nothing supplies it`), a rail with loads whose current the solver has not settled, and a converter whose efficiency it has not settled. The Problems dock lists the notes of a board grouped by their cause, with a count for each (the server says it for every note: `not_stated`, `part_model`, `unjudged`, `circular`, `supply`, `pick`, `track`, `other`), so a long list of notes reads as the few things to do about them: a rail nothing supplies and a load not settled are `supply`, a level or an input current a part's definition leaves out is `part_model`, a value nothing states is `not_stated`.

### The solved values

`netsmith board build --emit-solved <file>` also writes what the solve left each member of each board with, as JSON: the range of current an inductor carries (`reg.inductor.current`), the voltage a chain leaves at a node, the power in a resistor, and the values of the row each part was picked as. The file is an array with one object per board (`board`, and `System/Board` for a board of a system) and its `members`, each with its `path` from the board, a quantity's `unit` and its `range` in that unit (`[lo, hi]`, `"inf"` for an open end), a text's or an enum's `value`, and `shown`, the value as a message prints it (`5.047mA..50mA`). A member nothing gave a value is left out. A board that fails before the solve (a name that is not defined) leaves no record, so nothing is written for it; a board whose solve fails is written, as below.

The same file says why each part has its row. Each board has its `picks`, one for each part the solver chose a catalog row for, in the order it chose them: the part's `path`, the `catalog`, the `row`, a bank's `count` when it is more than one, and `passed`, the rows passed over before it. First come the rows before it in catalog order that do not fit what the design states (`unfit`, with the column and what the design states of it), then the rows the part's turn tried, in the order it tried them: `refused` with the first relation the row failed and `at`, the `file:line` of that relation; `later` when the row was accepted and the parts after it in its group could not all be chosen with it; `same` when it is the same as a row passed over before it for every relation; `skipped` when the search found without a trial that a trial would refuse it; `partly` when a bound that waits for a part still to be chosen holds for only some of its values. The turn is the one that made the pick, with the rows the parts before it have. A part a pick lock holds to its row has `held` (`the pick lock buck.lock.toml`) and nothing passed over, as no other row was tried: delete the lock, or run `netsmith project lock --update`, to see the rows the solver would take now. The solver keeps this record only for `--emit-solved`, so a build that does not ask is not slower.

A board whose solve fails has a record too, so a group that fails can be read and not only its error. Its object has `"failed": true`, the `members` and `picks` the solve got to (the parts of the groups that were chosen, and what the DC analysis wrote for the rest), and `unpicked`, one entry for each part of the group that failed: the `path`, the `catalog`, `held` as for a pick, `with`, the choices of the parts before it in the group that its turn was made with (`["a = 100 1%"]`; left out for the first part), and `passed`, every row its first turn passed over in the shape a pick has (a row `later` for a part that is not the first means the parts after it could not be chosen with it). A turn is the first one the search made when it reached the part, with the parts before it at the first rows that fit: what other choices of those parts would have tried is not there. A part that has no row that fits what the design states, or that every row is refused for whatever the other parts are, has its rows tried one at a time. With `--emit-solved` the file is written when a board fails if some board left such a record, and the exit code stays 1; `--emit-design` still writes nothing.

`netsmith board explain <path> [file.nsm]` prints the same record as text, for one part, module or member: `netsmith board explain reg.inductor buck.nsm` prints the row `reg.inductor` was picked as, what held it, the rows it passed over with why and where (`refused (at std/parts/regulator.nsm:603): reg.inductor.inductance must always be at least reg.ic.min_inductance, ...`), then each member at the path or inside it with the range the solve left it (`reg.inductor.current  5.047mA..50.06mA`). The path is a part, a module (`reg` is every part inside it, and `re` is nothing) or a member. It lists the first 20 rows a part passed over and counts the rest; `--all-rows` lists them all, and `--board` keeps one board of a file with several. When a board fails the error is printed as `check` prints it and the record of the failed group follows, with the exit code 1; it also exits 1 when nothing is at the path, and says which parts the solver chose rows for. Like `check` it writes nothing. A part the pick lock holds to a row has no rows passed over, as no other row was tried; run `netsmith project lock --update`, or delete the lock, to see the rows the solver takes now.
