---
name: netsmith-use-netsmith
description: The map of netsmith for an agent given a project (netsmith.toml, src/*.nsm) - the loop of commands, the files a run writes and the files you write, exit codes, JSON output, and which skill to read next. Use when you start in any netsmith project, and when you do not know which command or skill comes next.
---

# Use netsmith: the map

This skill says what comes after what, which file is whose, and which skill to read for each job. It repeats no flag: `netsmith --help` lists the nouns, `netsmith <noun> --help` its verbs, and `netsmith <noun> <verb> --help` every flag. The language guide is `docs/netsmith/LANGUAGE.md` (the index) and `docs/netsmith/language/` (a chapter for each construct; `docs/LANGUAGE.md` and `docs/language/` in netsmith's own repository). Its chapter 27 is the table of every command.

## The loop

Every board goes through the same steps. Do them in this order and say what each printed.

1. `netsmith project init` in an empty directory makes the project (skip it in an existing one). After a netsmith upgrade `netsmith project refresh` updates the agent files you have not edited.
2. Write or change `src/*.nsm`: the requirements as numbers, never part numbers (`netsmith-write-nsm`, `netsmith-design-a-board`).
3. `netsmith board check` checks and solves the boards and writes nothing. Read the first error only, fix it and run it again (`netsmith-read-errors`).
4. `netsmith board build` does the same and writes the schematic to `out/`, the pick lock and the pin file. Run it before you say a design works.
5. Look at what you made, with the commands that draw it; you read the picture with your image viewer:
   - `netsmith view schematic`: the schematic as a PDF, an SVG or a PNG.
   - `netsmith view 3d`: the board in 3D, with the heights of its parts.
   - `netsmith view pcb`: the copper in 2D, a layer and a region at a time, with the connections still open.
   - `netsmith view gerber`: the files of a fab package as the board house gets them.
   - `netsmith view editor` serves the editor for the person; with it open, `netsmith view editor-3d` drives the 3D view they are looking at.
6. `netsmith board fab` places and routes the board, runs the design rules check and writes `fab/`. For some boards and several copies, name an order in `netsmith.toml` and run `netsmith project order`.
7. Read `fab/<Board>/fab-notes.txt` and tell the person every note it raised (`netsmith-prepare-fab-order`). Never place an order.

## What to run for what

| You want to | Command | Skill |
|---|---|---|
| Know why a part was picked, keep a pick, choose another row, add a catalog part | `board explain`, `board swap`, `project lock`, `catalog` | `netsmith-pick-parts` |
| List the boards, systems, panels, files and orders, to learn the names `--board` and `--name` take | `project show` | `netsmith-design-a-board` |
| See the parts and nets with what the solve left them, the bill of materials, the power tree, a system, a panel, the copper areas, how the pours join each pad | `board show`, `board bom`, `board power`, `board system`, `board panel`, `board copper`, `board fill` | `netsmith-design-a-board` |
| Compare two designs, or the project with a git ref | `board compare` | |
| Say where parts are on the board | `layout` | `netsmith-use-layout` |
| State where parts must be for a case or a carrier | the physical file, then `board fab` | `netsmith-use-constraints` |
| Finish copper the router left open, keep copper a person drew, run the design rules check alone | `routes`, `board drc`, `board fab` with `--reuse-routes` | `netsmith-route-a-board` |
| Fix pins of a part's functions | `board pins` | `netsmith-design-a-board` |
| Arrange the schematic | `schematic` | `netsmith-design-a-board` |
| Bring in a DXF drawing, KiCad's libraries, a part from a symbol, a case's STEP model | `import` | `netsmith-design-a-board` |
| Move a KiCad board into the project | `import port` | `netsmith-port-a-kicad-board` |
| Split a repeated circuit into a module | | `netsmith-write-a-module` |
| Report what is worth changing in the sources | `project lint` | `netsmith-write-nsm` |
| Put the sources in the one layout of the language (CI checks it with `--check`) | `project fmt` | `netsmith-write-nsm` |

`dev` is for developing netsmith, not for working in a project.

## The files

The files you write, and commit with the source:

- `src/*.nsm`: the circuits (boards, modules, systems). `netsmith.toml` lists the files that hold boards (`top_level`); its optional `revision` is the revision of the project's boards, shown in the schematic's title block and at the head of `fab-notes.txt`; ask the person for it, do not invent one.
- `<stem>.physical.nsm` beside a source: what the case and the layout designer require (outline, cutouts, mounting holes, keepouts, where a part must be). It is never in the board itself.

The files a command writes, and you commit with the source (look at the diff, do not edit them to make an error go away):

- `<stem>.pcb.toml`: the layout file, where each part is. The editor and the placer write it, and so do `layout set` and `layout capture`.
- `<stem>.routes.toml`: copper fixed by hand or imported, written by `routes` and the editor.
- `<stem>.sch.toml`: how the schematic is arranged, written by `schematic` and the editor.
- `<stem>.pins.toml`: the pins a board's requests got, written by `board build` and `board fab` and held by `board pins`.
- `<stem>.lock.toml`: the pick lock, the parts the solver picked. `board build` writes it, and `project lock` refreshes it or checks in CI that it is current.
- `dependencies.lock.toml`: where a git dependency was read.

The files a run writes that you do not commit (git ignores them): `out/` (the schematic and its tables) and `fab/` (a package for each board: the Gerbers, `bom.csv`, `cpl.csv`, `fab-notes.txt`, the KiCad files and the 3D scene). To hand the KiCad files to someone, add `--portable` to `netsmith board build` or `netsmith board fab`: the board's folder then holds the libraries and 3D models it uses (`libs/`, `models/`), library tables with paths inside the folder and `LICENSES.md`, and opens in KiCad wherever it is moved; a 3D model that cannot be found is a `warning:` to tell the person. A board the router could not finish is written marked INCOMPLETE with `routing-failure.txt` and `routing-report.json` beside it. A part number a person chose is in the source, as a narrowing, and in no file of its own.

## Exit codes and JSON

- Every command exits 0 when it did what it was asked, 1 when what it checked failed (an error in the design, a lock that is not current), and 2 when it could not run (a usage mistake, a missing file). A run is not done while it exits 1.
- `--format json` is wherever there is output for a program: one JSON document on stdout and nothing else there. Use it to read findings, the bill of materials, a layout summary. Messages for a person go to stderr.
- `--progress json` prints the progress of `board fab`, `project order` and `dev board3d` as JSON lines on stderr, so a program can follow a long run (placing, routing) without parsing text.
- `NETSMITH_TIMINGS=1` prints the time of each stage to stderr.

## An agent host

`netsmith mcp` serves these commands to a host that speaks the Model Context Protocol (Claude Code, Claude Desktop, an editor): each command is a tool named by its words (`board_check`), the project's files and the guide are resources, and these skills are prompts. In Claude Code, in the project's folder: `claude mcp add netsmith -- netsmith mcp`. It adds nothing the command line lacks; the guide's chapter 27 says how it runs a call.

## What to ask the person

Ask before you go on; do not guess:

- The requirements you lack, as numbers: supply voltage with tolerance, each load's current range, connectors, layers, the board's size, the factory.
- Anything a person decides: which part to hold when more than one fits, who assembles which part (`assembly`), a revision, the boards and the copies of an order.
- A decision about the language or its semantics (syntax, what a check means, a new construct): present the options and do not choose.

The three rules of the project's `AGENTS.md` (ADR 0015) hold throughout:

1. Run `netsmith board build` before you say a design works, and say what it printed.
2. A catalog part needs a source for its number. Never write a part number from memory.
3. Ask the person before a decision about the language or its semantics.

## Done when

`netsmith board build` prints no error and no `warning:` you have not told the person about, `netsmith board fab` ends with the design rules check passed for every board, and you have listed for the person every note of `fab-notes.txt`.
