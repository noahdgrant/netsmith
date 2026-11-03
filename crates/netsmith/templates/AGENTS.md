# Working in this project

This is a netsmith project: circuits as code. Boards are written in `.nsm` files under `src/`; `netsmith` checks them, picks real parts that work in the worst case, and writes KiCad files and a package for JLCPCB. `netsmith.toml` lists the files that hold boards.

## Commands

- `netsmith build`: check and solve the boards, write a KiCad schematic to `out/`.
- `netsmith editor`: the source editor in a browser, with the compiler's messages inline and the schematic, power tree, bill of materials and PCB beside it.
- `netsmith fab`: place and route, run the design rules check, write the files to upload to `fab/`.
- `netsmith catalog age` lists the rows of the project's `catalogs/*.csv` whose price and stock (`date`) are more than 90 days old; read JLCPCB's parts page and edit them by hand. `netsmith catalog refresh --dry-run` shows what checking the rows against LCSC's record would write: only the day in a `checked` column (if the file has one), never a price, stock or library. JLCPCB data is not fetched, and LCSC's terms of use are unread. It is opt-in and uses the network; nothing else does.

## Where to read

- `docs/netsmith/LANGUAGE.md` is the language guide: the syntax, a runnable example and the real error for each construct, and a catalogue of the standard library. Read it before you write `.nsm`.
- `.agents/skills/` has the netsmith skills (`.claude/skills` links to it). Use the one that matches the task:
  - `netsmith-design-a-board`: requirements to a fab package.
  - `netsmith-write-nsm`: a cheat sheet for `.nsm`.
  - `netsmith-write-a-module`: splitting a circuit into modules and interfaces.
  - `netsmith-read-errors`: what each message means.
  - `netsmith-use-constraints`: where parts must be (a case, a carrier) and the findings when they are not.
  - `netsmith-prepare-fab-order`: `fab-notes.txt` and the order preview checklist.
  - `netsmith-port-a-kicad-board`: porting a KiCad design a sheet at a time.

## Rules

- Run `netsmith build` before you say a design works. Say what it printed.
- A catalog part needs a source for its number. Do not write a part number from memory. The project's own parts are rows of `catalogs/<catalog>.csv`, listed under `catalogs` in `netsmith.toml`; a row that names a part has a `source` cell.
- Ask the user before a decision about the language or its semantics. Present the options; do not choose.
