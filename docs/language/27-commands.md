## Commands

`netsmith --help` and `netsmith <noun> <verb> --help` document every command and flag. The skill `netsmith-use-netsmith` is the map for an agent: the loop of commands, the files, the exit codes and which skill fits which job.

### The command line

The commands are grouped under nouns, each with its verbs (issue #1090): `netsmith <noun> <verb>`, so `netsmith --help` lists nine nouns and `netsmith board --help` the verbs of a board.

- `project`: the project as a whole: make one (`init`), bring its agent files up to this netsmith (`refresh`), refresh or check its pick lock (`lock`), report what is worth changing in its sources (`lint`), make one of its orders (`order`).
- `board`: what is done to the boards of a file or of the project: `check`, `build`, `fab`, `drc` alone, `explain` a pick, `compare` two designs or the project with a git ref, `swap` a part for another row, `pins` (show, lock, move), `copper` (the planes, pours, hand zones and teardrop fills), and the views of the solved design the editor shows, printed as text or `--format json`: `bom`, `power`, `system`, `panel`.
- `layout`: the layout file, where each part is.
- `routes`: the routes file, the copper fixed by hand or imported.
- `schematic`: the schematic layout file (`<stem>.sch.toml`), how the schematic is arranged: `show` it, `set` an entry, `check` it against the board, `arrange` the sheets of a big board (issue #1093). It is `layout`'s counterpart for the drawing: a noun is the thing a file holds, so the PCB's solution is `layout` and the schematic's arrangement is `schematic`, and the picture of it is the verb `view schematic`.
- `view`: look at a board: `3d` draws it to a PNG, `schematic` writes its schematic as a PDF, an SVG or a PNG, `pcb` draws its copper in 2D to a PNG or an SVG, `gerber` draws the files of its fab package, `editor` serves the editor, `editor-3d` drives the 3D view of a running editor.
- `catalog`: the rows of the catalogs, checked against LCSC's record.
- `import`: what comes into a project from outside: a DXF drawing (`dxf`), KiCad's library tables (`kicad-libs`), a part from a symbol (`part`), a port of a KiCad board (`port`), a case's STEP model (`case`).
- `dev`: the tools for developing netsmith and checking ports: `route` routes a saved placed board with the router's tuning flags, `parity` compares a port with its original, `board3d` prints the 3D scene's report.

The names follow four rules:

- One name for one thing. A command is named by the noun it acts on and its verb, and one verb is never a second spelling of another. A command or flag of an earlier release is an error that names the one to write and prints the whole line rewritten (ADR 0001; `docs/MIGRATING.md`).
- `--out` is always a folder. A command that writes one file names the flag after the file: `view 3d --picture` and `view editor-3d --picture` (a PNG), `view schematic --picture` (the PDF, SVG or PNG), `view pcb --picture` (the PNG or SVG), `layout export --kicad-pcb`, `dev board3d --glb` and `--step`, and `--write` for the source an import writes.
- One way to choose a board: the source file (`FILE.NSM`; with none, the project's boards) and `--board NAME` for one board of a file with several. Where the command takes another file first (`routes import BOARD.KICAD_PCB`, `layout set PATH`), the source is `--file FILE.NSM`.
- `--format json` wherever there is output for a program to read: one JSON document on stdout, and nothing else there.

Each command, what it was before, and its flags (a test walks `--help` of every command and holds this table to it, `crates/netsmith-cli/tests/cli_table.rs`):

| Command | Before | Flags |
|---|---|---|
| `netsmith project init` | `netsmith init` | `--name` |
| `netsmith project refresh` | `netsmith init --refresh` | none |
| `netsmith project lock` | `netsmith lock` | `--update`, `--check`, `--part`, `--except` |
| `netsmith project lint` | new | `--strict`, `--format` |
| `netsmith project order` | `netsmith fab --order [NAME]` | `--out`, `--route-timeout`, `--rip-rounds`, `--allow-drc-violations`, `--cross-check`, `--strict`, `--locked`, `--constraints`, `--no-glb`, `--no-step`, `--no-pdf`, `--offline`, `--quiet`, `--place-seeds`, `--place-budget`, `--progress`, `--to-editor` |
| `netsmith board check` | `netsmith check` | `--strict`, `--format`, `--notes` |
| `netsmith board build` | `netsmith build` | `--out`, `--strict`, `--locked`, `--emit-design`, `--emit-solved`, `--format`, `--notes` |
| `netsmith board fab` | `netsmith fab` | `--out`, `--route-timeout`, `--rip-rounds`, `--allow-drc-violations`, `--cross-check`, `--strict`, `--locked`, `--constraints`, `--pins`, `--no-glb`, `--no-step`, `--no-pdf`, `--save-placed`, `--reuse-routes`, `--offline`, `--quiet`, `--place-seeds`, `--place-budget`, `--progress`, `--to-editor`, `--format`, `--notes` |
| `netsmith board drc` | new | `--board`, `--kicad-pcb`, `--place-seeds`, `--place-budget`, `--format` |
| `netsmith board explain` | `netsmith explain` | `--board`, `--all-rows` |
| `netsmith board compare` | `netsmith compare` | `--ref`, `--against-ref`, `--board`, `--map`, `--by-refdes`, `--ignore-unconnected`, `--loose-values`, `--format` |
| `netsmith board pins show` | new | `--file`, `--board`, `--format` |
| `netsmith board pins lock` | new | `--file`, `--board`, `--format` |
| `netsmith board pins unlock` | new | `--file`, `--board`, `--format` |
| `netsmith board pins move` | new | `--file`, `--board`, `--format` |
| `netsmith board pins apply` | new | `--proposal`, `--file`, `--board`, `--format` |
| `netsmith board swap` | new (the editor's Swap) | `--board`, `--apply`, `--format` |
| `netsmith board show` | new | `--board`, `--format` |
| `netsmith board bom` | new | `--board`, `--offline`, `--format` |
| `netsmith board power` | new | `--board`, `--format` |
| `netsmith board system` | new | `--name`, `--format` |
| `netsmith board panel` | new | `--name`, `--format` |
| `netsmith board copper` | new | `--board`, `--format` |
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
| `netsmith view schematic` | new (#1091) | `--board`, `--format`, `--sheet`, `--picture` |
| `netsmith view pcb` | new (#1086) | `--board`, `--layer`, `--side`, `--region`, `--scale`, `--format`, `--picture`, `--highlight`, `--only-open`, `--courtyards`, `--designators`, `--drc`, `--route`, `--route-timeout`, `--place-seeds`, `--place-budget` |
| `netsmith view gerber` | new (#1105) | `--layer`, `--region`, `--scale`, `--format`, `--picture` |
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
| `netsmith dev route` | `netsmith route` | `--out`, `--route-timeout`, `--rip-rounds`, `--margin`, `--turn`, `--tries`, `--stages`, `--plan`, `--classes`, `--arcs` |
| `netsmith dev parity` | `netsmith parity` | `--table`, `--parts`, `--svg`, `--format` |
| `netsmith dev board3d` | `netsmith board3d` | `--board`, `--format` (was `--json`), `--glb`, `--step`, `--place-seeds`, `--place-budget`, `--progress`, `--to-editor` |
| `netsmith mcp` | new | `--project` |

The place left for a command to come, named by the same rules: `project fmt`.

### The MCP server

`netsmith mcp` offers the command line to an agent host that speaks the Model Context Protocol (Claude Code, Claude Desktop, an editor), on stdin and stdout (issue #1106). The host starts it, and it serves until the host closes its stdin. It adds nothing the command line lacks:

- Each command is a tool, named by its words (`board check` is `board_check`, `view editor-3d` is `view_editor_3d`), with an argument for each of its flags and positional arguments, under the name of the field clap gives it (`place_seeds` for `--place-seeds`, `file` for `FILE.NSM`), and with its type, choices and default. The list is short, because a host sends all of it to the model (issue #1133): a tool has the first line of its command's help and an argument the first line of its help. The whole `--help` of a command, what `netsmith board fab --help` prints, is a resource, `netsmith://help/board-fab` (the command's words joined by `-`), listed with the other resources, for an agent to read when it needs it. The tools and the help resources are read from the command line itself, so a new command or flag is a tool, an argument and a help resource with no change to the server. `view editor`, a server that never ends, is not a tool, nor is `mcp`.
- A call runs the command in the project's folder, `--project DIR` or, with no flag, the folder the server was started in, as a person runs it there: the commands that write files (`board build`, `board fab`, `layout set`) write them. Where a command has `--format json` the call asks for it unless its `format` says `text`, and the result is the command's JSON document, as text and as structured content; otherwise the result is the command's text. What it says on stderr follows. An exit code of 1 (what the command checked failed) or 2 (it could not run, a mistake in the arguments included) makes the result an error, with the code.
- Each line the command says on stderr is a progress notification when the host asks for progress, and `--progress json` is asked for where the command has it. A call the host cancels stops its command as Ctrl+C does in a terminal, and is not answered (ADR 0062).
- The resources are the project's files (`netsmith://project/src/main.nsm`: its source and the other text files netsmith reads and writes, from the nearest folder up from the project's folder with a `netsmith.toml`; a path outside it is not read) and the language guide (`netsmith://guide/LANGUAGE.md` and its chapters, `netsmith://guide/language/27-commands.md`). The prompts are the skills `netsmith project init` writes, one for each, with an optional `request` that follows the skill's text.

To use it from Claude Code, in the project's folder:

```sh
claude mcp add netsmith -- netsmith mcp
```

or, to share it with everyone who opens the project, a `.mcp.json` beside `netsmith.toml`:

```text
{ "mcpServers": { "netsmith": { "command": "netsmith", "args": ["mcp"] } } }
```

Another host that starts a server on stdio takes the same: the command `netsmith`, the argument `mcp`, started in the project's folder. A host that cannot say the folder (Claude Desktop's `claude_desktop_config.json` has no such key) gives it as `--project`, which works the same in every host:

```text
{ "mcpServers": { "netsmith": { "command": "sh", "args": ["-c", "cd /path/to/project && exec netsmith mcp"] } } }
```

```text
{ "mcpServers": { "netsmith": { "command": "netsmith", "args": ["mcp", "--project", "/path/to/project"] } } }
```

### The editor's views on the command line

`netsmith board show`, `board bom`, `board power`, `board system` and `board panel` (issues #1096 and #1137) print the views of the solved design that the editor draws, as text or, with `--format json`, as the document the editor's own route answers: `GET /api/board`, `/api/bom`, `/api/power`, `/api/system` and `/api/panel`. They are made by the code those routes call (`netsmith_workspace::Workspace`'s `Backend`), so the command line and the editor cannot differ, and each is read-only: it writes no file and needs neither `fab` nor a browser.

```sh
netsmith board show --board main          # the parts with what the solve left them, the nets with their pins, the pick lock
netsmith board bom --offline              # what the bill costs: lines, price, stock, total
netsmith board power --board main         # the current on each rail, what each supply leaves
netsmith board system --format json       # a system's boards, joins, mates and grounds
netsmith board panel                      # a panel's grid, size and checks
```

- `board show` prints the board's parts and nets as the editor's inspector shows them: each part with its designator, kind, value, package, path, LCSC number, whether it is assembled, the declaration that made it and the scopes round it, what the solver worked out for it (effective capacitance and bias, tolerance, ratings, the catalog row), the pins its pin requests got and where its package came from; each net with its pins (the pad, and the datasheet's name and the function the board chose); the board's notes; and whether the pick lock beside the source is current (what `project lock --check` says, which it only reads).
- `board bom` prints the lines the board house assembles (grouped as `bom.csv` is) and those soldered by hand, each with its price, stock and the day they were read, and the total of the priced lines; a line whose price is unknown says why, and a generic part no catalog row stands for says what is wanted. `--offline` asks no supplier, as `board fab --offline` does (ADR 0047).
- `board power` prints each rail with its voltage, the supplies and loads on it with their currents, what the loads add up to against what the supply can give, the converters between rails with their efficiency, and the power issues (an overdrawn supply, a rail nothing supplies, a load or an efficiency not settled). It exits 0 with issues: `board check` and `board build` are the commands that fail on them.
- `board system` prints a `system`'s members with their ports, the joins between them, the mated connectors with each board's role and net at each contact, and the grounds the mates join; named after a board, the members of that board and the nets between them.
- `board panel` prints a `panel`'s board, grid, cell and panel size, rails and the panel checks; the drawing is only in the JSON (`svg`).

`show`, `bom` and `power` take the source file (or, with none, the project's boards) and `--board NAME`; `system` and `panel` take `--name NAME`. A name is as the editor names it, `System/Board` for a board of a system, and `Board` alone when no other board has that name. With one board (one system, one panel) the name may be left out; with several, the command exits 2 and lists them. The exit code is 0 when the view is printed, 1 when the source does not build (with its errors), and 2 when the command cannot run.

### Every editor route has a command

An agent must be able to drive and read everything a person does in the editor (issue #1098). A test holds the editor to that: `crates/netsmith-cli/tests/editor_routes.rs` reads the routes of the editor's server (`crates/netsmith-editor/src/server.rs`, the `.route("/api/...")` table) and has a row for each method of each route, which says one of three things:

- the command (or commands) that does the same through the same code, by the words clap's model names it by (`board pins lock` for `POST /api/pins/lock`). The test fails when a command named there does not exist;
- the command that is still to come, with the issue that builds it (`GET /api/pcb/pours` is `view pcb`, issue #1086, which draws the 2D picture of the copper). The test accepts the row until a command of that name exists and then fails, so that the row is changed to name it;
- why the route is an affordance of the page and needs no command, in one line: the previews of a track being drawn (`/api/pcb/walk`, `/api/pcb/shove`, `/api/pcb/reroute`), the socket (`/api/events`) and the progress post (`/api/progress`), the page's reads and writes of the files of the text editor and the project tree (`/api/file`, `/api/fs/...`), the cancels (a command stops with Ctrl+C, an MCP call with the host's cancel), and the choice of the page's analysis (`/api/analysis/run` and `/api/analysis/auto`).

A route of the server with no row fails the test, and so does a row with no route, so a pull request that adds a route to the editor adds its command, or the reason it has none, in the same change. The audit that made the first table found two routes whose command was not there: the board's parts and nets as the inspector shows them (`GET /api/board`, issue #1137, which is `board show`) and the project's boards, systems, panels and files (`GET /api/project`, issue #1138).

### The editor

`netsmith view editor` serves the editor on `127.0.0.1` and prints the address to open: a project tree, the source with the compiler's and solver's messages beside the lines they are about, and views of the selected board (schematic, power tree, system, bill of materials, PCB) with buttons for `build` and `fab` (ADR 0019). A board of more than 16 parts is drawn in a box for each module instance, and the boxes, and the cards of the overview of all modules, are set out by how the modules connect: modules joined by nets come one after the other, the same way every time. A card of the overview dragged onto another takes that card's place: the editor writes the sheet's place to the schematic layout file below (`at = [x, y]` in the sheet's table, the top-left corner of its box in millimetres), and the sheets never moved are set out by their connections round it (ADR 0139). A rail is drawn with the ground and supply symbols at its pins. Errors and warnings are listed first, and the notes (relations not evaluated, balances left incomplete) in a closed section. It checks the text as you type without writing it, saves with `Ctrl+S`, reads a file again when it changes on disk, and with no file acts on the project like `build`.

In the schematic, a part can be dragged on the grid, nudged with the arrow keys, turned with `r` and mirrored with `m`. The editor writes where it was put to `<stem>.sch.toml` beside the source file (`src/main.nsm` has `src/main.sch.toml`), so the drawing stays as arranged across edits; the file never holds a wire or a net, an entry that no longer matches the design is listed in the problems and ignored, and deleting the file gives the automatic drawing back (ADR 0061). The editor can also compare the project with a git ref, or with a second ref, and say what changed in the circuit (parts, nets, rails, systems, the PCB, the bill of materials, problems) and not which lines (ADR 0059; `netsmith schematic` reads and writes the same file from the command line (`show` lists its entries and the boxes of the sheets, `set stage.output_cap --by 2.54,0 --rotate 90` writes an entry and keeps the file's comments, `set stage --at 101.6,0` places a sheet, `check` lists the entries that name nothing or that the drawing could not honour, `arrange --write` keeps the arrangement of the overview as `at` entries)). `netsmith board compare --ref <GIT REF>` does the same from the command line, with the editor's code: it finds the project from the current folder, reads the ref as the editor does (all the project's files, its catalogs and its dependencies' lock), and prints the changes as text, or as the editor's own `GET /api/compare` document with `--format json`; `--against-ref` takes a second ref in place of the files on disk, and `--board` one board. It exits 0 when the circuit is the same, 1 when it differs and 2 for a mistake (not in a git work tree, a ref that does not exist, a project that does not build at the ref). An analysis, a comparison, a build and `fab` can be cancelled with their Cancel button; a cancelled analysis leaves the last reading (ADR 0062).

The editor's bill of materials marks the lines whose price and stock are old (as `netsmith catalog age` lists them), says `unknown` for the rest, and has a "Check parts against LCSC" button for the project's own catalogs.

### The design rules check alone

`netsmith board drc [file.nsm]` runs netsmith's own design rules check (ADR 0037), the one `fab` runs and the editor's PCB view shows, with nothing routed and no package written. It checks the board as `fab` has it before the router: the placement with the copper of the routes file on it, so a connection the file does not make is an `unconnected_items` error (a board with no routes file has one for each), and the design's requirements on where parts go are measured as for `fab`. `netsmith board drc --kicad-pcb board.kicad_pcb` checks a KiCad board instead (the `.kicad_pro` beside it is its rules), such as one `layout export` wrote or one that came back before `layout import` or `routes import`; it has none of the process's extras that `fab` adds (the fiducials, the stackup, the mask web), so it can find less than `fab` does of the same board. The findings are the lines `fab` lists (its `fab-notes.txt` has the warnings of the same board), errors first; the exit code is 1 when there is an error and 0 when there is none, whatever the warnings. With `--format json` each finding has its `severity`, the `code` `drc`, the `rule` (`clearance`, `short_circuit`, ...), the `place` (`x`, `y` in mm in the frame of the board file, y down, and the `layer`), the `nets` and the `parts` it names, and the board's `outline_at` says where the outline's top left corner is in that frame (a place less it is in the frame of the layout and routes files), so that the places can be drawn as markers on a picture of the board.
### The schematic as a document
`netsmith view schematic [FILE.NSM]` writes the schematic the editor draws, with the schematic layout file beside the source honoured, as a file to hand on (issue #1091): `--format pdf` (the default), `svg` or `png`, `--board NAME` for one board of a file with several (a system's name writes the system), `--sheet NAME` for one sheet, and `--picture FILE` for the file (by default `<board>-schematic.<pdf|svg|png>` in the current folder; the path is printed). netsmith writes it itself, with no KiCad and no browser, and the same source gives the same bytes:
- **PDF.** A page for each sheet: a module's box of a board drawn by module (`power`, `stage (2/3)`), or the whole drawing of a board of 16 parts or fewer, named for the board. Each page is sized to its sheet, with a title block that names the sheet (with `Rev A` beside it at the right when `netsmith.toml` has `revision = "A"`, chapter 26), the board, the project and the release of netsmith, and `Sheet 3 of 7`. It is vector drawing with the text in Helvetica (Courier for a note's relation), which every PDF reader has, so no font is in the file and its text can be searched. A system's PDF begins with the system's own sheet, its boards and the joins between them, then has each member's sheets.
- **SVG.** The drawing the editor shows, or with `--sheet` the same drawing framed on that sheet.
- **PNG.** The drawing filled by netsmith's own rasterizer at 10 pixels a millimetre (fewer for a drawing longer than 8192 pixels), its text in netsmith's stroke font: a picture an agent can look at.
`board fab` writes the PDF into each board's package as `<board>-schematic.pdf`, beside the `.glb` and outside the Gerber zip, and a system's as `<System>/<System>-schematic.pdf`; `--no-pdf`, or `pdf = false` under `[project]` in `netsmith.toml`, leaves it out and removes an earlier run's. A schematic that cannot be drawn is a warning in `fab-notes.txt`, never a failed `fab`. The editor's schematic has `PDF` and `SVG` buttons, and the commands "Export schematic as PDF" and "Export schematic as SVG", which save the same bytes.

### The copper as a picture

`netsmith view pcb [FILE.NSM]` draws a board's copper in 2D, as the editor's PCB view does, to a PNG or an SVG (`--format png|svg`, the same drawing in both; issue #1086), with no editor, no browser, no GPU and no KiCad; the same board and flags give the same bytes. It is the picture for routing by hand or by an agent: where the tracks, vias and pours of a region are, which connections are still open, and what is in their way.

- **The copper** is the copper of the routes file beside the source and the router's copper the editor keeps (`.netsmith/routes/`) for every net whose pads have not moved since it was routed: what `board fab --reuse-routes` starts from. Nothing is routed unless `--route` is given, which runs the router first from that copper (`--route-timeout`, as for `fab`); the picture is the only file written. A pour is drawn as the Gerbers fill it.
- **The open connections** are what that copper leaves, the connections `routes show` lists as left to route: bright straight lines from pad to pad, printed after the path (`open: net GND: D1.1 to J1.2`). `--only-open` lights their nets and fades the rest; `--highlight net:GND,R1` lights nets and parts (a part by its designator or its path) as `view 3d --highlight` does.
- **A layer, a side, a region.** `--layer top|bottom|inner1|...|all` (all by default, the layer nearest the eye drawn last); `bottom` is seen from below, mirrored left to right as the editor's Side switch shows it, and `--side top|bottom` says the side for any layer. `--region X,Y,W,H` draws only that rectangle, in mm from the outline's top left corner (the frame of the layout file and the routes file, seen from above also when the board is drawn from below), and `--scale` is the pixels a millimetre (20 by default; a picture is at most 16384 pixels a side).
- **More marks.** `--courtyards` and `--designators` draw the parts' courtyards and designators (of the face of an outer layer drawn alone, else of every part); `--drc` runs the design rules check of the board drawn (`board drc`'s) and marks each finding's place with a numbered ring, red for an error and amber for a warning, and prints the findings by number (`marker 1: error [clearance] ...`). A missing connection is no marker: it is an open connection.

```sh
netsmith view pcb --region 10,5,20,15 --scale 40 --only-open   # main-pcb.png: a region, its open connections lit
netsmith view pcb --layer bottom --designators --drc            # main-pcb-bottom.png: the bottom seen from below
netsmith view pcb --route --format svg --picture routed.svg     # route first, then draw the result
```
`netsmith view gerber PACKAGE` reads the Gerber (RS-274X) and Excellon drill files of a fab package and draws them as the board house will get them, with netsmith's own reader and no KiCad (issue #1105): the files `board fab` wrote (the zip, or `board fab`'s output folder of a board, which opens as its `<board>-gerbers.zip`), or a package another tool made, zipped or in a folder. Each file's function is taken from its X2 attribute (`%TF.FileFunction`), or else from its name (KiCad's `<board>-F_Cu.gbr`, `-In1_Cu`, `-Edge_Cuts`; Protel's `.GTL`, `.GBL`, `.G1`, `.GTS`, `.GTO`, `.GTP`, `.GKO`; Eagle's `.cmp`, `.sol`); a file neither names is drawn only with `--layer all`. A file with a construct the reader does not support (a step and repeat `%SR`, a block aperture, an arc routed in a drill file) is never drawn: it is named on stderr with its line and command, the rest is drawn, and the exit code is 1.
- `--layer top` (the default) draws the top copper, the mask's openings, the paste, the silkscreen, the outline and the holes, as a person sees the top; `bottom` the same of the bottom, mirrored as a person sees it; `inner<n>` that inner copper layer with the outline and the holes; `all` every layer, seen from the top. Each file is drawn in the colour of its function, the objects of each combined as its polarity says (a clear object takes away from what is under it in its own file, not from the other files).
- `--region X,Y,W,H` draws that rectangle, in mm from the outline's top left corner, y down (the frame of the layout and routes files: the corner is the box of the outline file's centrelines), and `--scale` the PNG's pixels a millimetre (20 by default, fewer for a picture longer than 8192 pixels). With no region it draws everything the layer has, with a millimetre round it.
- `--format png` (the default) or `svg` writes the picture to `--picture FILE` (by default `<package>-<layer>.png` in the current folder) and prints its path. The PNG is the SVG filled by netsmith's own rasterizer, and the same package gives the same bytes.
- `--format json` prints a summary of every file instead: the `origin` of the frame in the files' coordinates, the `board`'s size (the outline's), the `extent` of everything drawn, the `views` `--layer` can draw, and for each of the `layers` its `file`, `format`, `function` (`copper`, `mask`, `paste`, `silk`, `outline`, `drill` or `other`), `layer`, how it was `identified_by`, its X2 `file_function` and `polarity`, whether it was `read`, its `extent`, and for a Gerber its `apertures`, `flashes`, `strokes`, `regions`, the objects drawn `clear` and the `area` it covers; for a drill file its `holes` and `slots`. Then the `holes` of the package by size (`diameter`, `plated`, `holes`, `slots`), the files not read (`unread`, with the `line` and the `message`), and the files that are neither Gerber nor Excellon (`skipped`).
The exit code is 0 when every file was read, 1 when one could not be, and 2 when the command cannot run (no such package, a layer the package has no file for, a picture of more than 16384 pixels a side).

### Power findings

The power model's findings come from `netsmith`, not from the editor's page: `build`, the editor's analysis (its `power` stage, the Problems dock's `Power` group) and the power tree it draws all carry the same ones. A rail that can draw more than the supply that states what it can give (`max_current`) is an error that stops the build, where the solver had no relation to check it against; `--strict` is not needed. The notes do not stop it, with or without `--strict`: a rail nothing supplies (`V5 has no source: nothing supplies it`), a rail with loads whose current the solver has not settled, and a converter whose efficiency it has not settled. The Problems dock lists the notes of a board grouped by their cause, with a count for each (the server says it for every note: `not_stated`, `part_model`, `unjudged`, `circular`, `supply`, `pick`, `track`, `other`), so a long list of notes reads as the few things to do about them: a rail nothing supplies and a load not settled are `supply`, a level or an input current a part's definition leaves out is `part_model`, a value nothing states is `not_stated`.

### The solved values

`netsmith board build --emit-solved <file>` also writes what the solve left each member of each board with, as JSON: the range of current an inductor carries (`reg.inductor.current`), the voltage a chain leaves at a node, the power in a resistor, and the values of the row each part was picked as. The file is an array with one object per board (`board`, and `System/Board` for a board of a system) and its `members`, each with its `path` from the board, a quantity's `unit` and its `range` in that unit (`[lo, hi]`, `"inf"` for an open end), a text's or an enum's `value`, and `shown`, the value as a message prints it (`5.047mA..50mA`). A member nothing gave a value is left out. A board that fails before the solve (a name that is not defined) leaves no record, so nothing is written for it; a board whose solve fails is written, as below.

The same file says why each part has its row. Each board has its `picks`, one for each part the solver chose a catalog row for, in the order it chose them: the part's `path`, the `catalog`, the `row`, a bank's `count` when it is more than one, and `passed`, the rows passed over before it. First come the rows before it in catalog order that do not fit what the design states (`unfit`, with the column and what the design states of it), then the rows the part's turn tried, in the order it tried them: `refused` with the first relation the row failed and `at`, the `file:line` of that relation; `later` when the row was accepted and the parts after it in its group could not all be chosen with it; `same` when it is the same as a row passed over before it for every relation; `skipped` when the search found without a trial that a trial would refuse it; `partly` when a bound that waits for a part still to be chosen holds for only some of its values. The turn is the one that made the pick, with the rows the parts before it have. A part a pick lock holds to its row has `held` (`the pick lock buck.lock.toml`) and nothing passed over, as no other row was tried: delete the lock, or run `netsmith project lock --update`, to see the rows the solver would take now. The solver keeps this record only for `--emit-solved`, so a build that does not ask is not slower.

A board whose solve fails has a record too, so a group that fails can be read and not only its error. Its object has `"failed": true`, the `members` and `picks` the solve got to (the parts of the groups that were chosen, and what the DC analysis wrote for the rest), and `unpicked`, one entry for each part of the group that failed: the `path`, the `catalog`, `held` as for a pick, `with`, the choices of the parts before it in the group that its turn was made with (`["a = 100 1%"]`; left out for the first part), and `passed`, every row its first turn passed over in the shape a pick has (a row `later` for a part that is not the first means the parts after it could not be chosen with it). A turn is the first one the search made when it reached the part, with the parts before it at the first rows that fit: what other choices of those parts would have tried is not there. A part that has no row that fits what the design states, or that every row is refused for whatever the other parts are, has its rows tried one at a time. With `--emit-solved` the file is written when a board fails if some board left such a record, and the exit code stays 1; `--emit-design` still writes nothing.

`netsmith board explain <path> [file.nsm]` prints the same record as text, for one part, module or member: `netsmith board explain reg.inductor buck.nsm` prints the row `reg.inductor` was picked as, what held it, the rows it passed over with why and where (`refused (at std/parts/regulator.nsm:602): reg.inductor.inductance must always be at least reg.ic.min_inductance, ...`), then each member at the path or inside it with the range the solve left it (`reg.inductor.current  5.047mA..50.06mA`). The path is a part, a module (`reg` is every part inside it, and `re` is nothing) or a member. It lists the first 20 rows a part passed over and counts the rest; `--all-rows` lists them all, and `--board` keeps one board of a file with several. When a board fails the error is printed as `check` prints it and the record of the failed group follows, with the exit code 1; it also exits 1 when nothing is at the path, and says which parts the solver chose rows for. Like `check` it writes nothing. A part the pick lock holds to a row has no rows passed over, as no other row was tried; run `netsmith project lock --update`, or delete the lock, to see the rows the solver takes now.

`netsmith board swap <part> [file.nsm]` is the editor's swap of a generic part that no catalog row stands for (_Catalogs_, issue #1094): it lists the nearest rows, each with what the board's checks say with it in the part's place and the change to the source that chooses it, and `--apply <row>` writes that change (a hand pick, ADR 0122), with the rest of the file as it was and the part picked again in the pick lock if the source has one. The part is a designator (`R1`) or a path (`r1`); the row is its name, its LCSC number or its number in the list. It reads the project as the editor does and asks the editor's own swap, so `--format json` is what `GET /api/swap` answers, and with `--apply` the row written and what `POST /api/swap` answers. It exits 1 when the board does not build or the part has nothing the swap can do (a row stands for it, or its value is stated where one edit would change other parts too).
