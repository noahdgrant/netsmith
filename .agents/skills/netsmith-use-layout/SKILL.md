---
name: netsmith-use-layout
description: Place, turn, flip and lock parts of a board in its layout file with `netsmith layout`, let the placer place the rest, and check the result. Use when a user says where a part is (or was moved to), asks to keep a placement, or when `layout check`, `fab` or the editor reports a layout file entry.
---

# Use the layout file

The layout file, `<stem>.pcb.toml` beside the source, holds the **solution**: where each part is, its turn, its face and whether it is locked. The physical file holds the **requirements** on where a part may be (`netsmith-use-constraints`); a requirement never goes in the layout file, nor a solved position in the physical file. The guide's section "The layout file" (`docs/language/23-the-shape-of-a-board.md`) has the format, the frame and the seed rules. `netsmith layout --help` and its subcommands' help have the flags.

A part with no entry is placed by the placer round the parts the file places; a part with `lock` is left where it is; any other entry is where the placer starts from, and moving a part by hand moves nothing else.

## Loop

1. **Read what is there**: `netsmith layout show --format json` (a PATH, a part's path or a module's, narrows it; `--board NAME` and `--into FILE.NSM` pick the board). It gives each part's `path`, `refdes` and `entry` (`at`, `turn`, `side`, `lock`, `source`, `line`); a part without `entry` is the placer's. A path the board lacks is refused with the nearest names. Paths are the design's (`stage.output_cap`, `decoupling[2]`), never a designator: a renumbered designator changes no path.
2. **Decide what is the user's**. Lock a part when the user, a drawing or a mechanical fit fixes it (a connector at the case edge, a part under a button). Leave every other part with no entry or unlocked, so the placer keeps shaping the board round them. A board with every part locked has nothing left to place.
3. **Write the entry**: `netsmith layout set <path> --at X,Y --turn 90 --side top --lock`. Use `set`, not the TOML: it reads and writes the file as the editor does, refuses a bad path or turn with the file untouched, and keeps the other tables (a comment in the file is not kept; tell the user when the file had any).
   - Frame: millimetres from the outline's top left corner, x right, y down. `--at` is the **footprint origin**, not the body or its centre; a requirement measures the body, so convert from a drawing's body numbers with the footprint's own offsets, and let `layout check` confirm.
   - `--turn` is any angle from 0 up to 360 (90, 45), counterclockwise as seen on screen from above; the placer turns by right angles only.
   - `--side bottom` is the top footprint mirrored left to right, then turned as seen from above.
   - A new entry needs `--at`; on an existing entry a flag left out keeps its value, and `--unlock` clears the lock. A new entry is `source = "hand"`.
4. **Check**: `netsmith layout check` (exit 1 on an error, notes do not change it; `--format json` has `ok` and `findings` with `severity`, `kind`, `path`, `line`, `message`). It places nothing and needs no KiCad. Kinds: `no_part` (an entry the board has no part for: the part was renamed or removed; fix or delete the entry), `bad_turn`, `bad_place`, `requirement` (a locked entry breaks the physical file, in `fab`'s words), `no_entry` (a note), `unmeasured`. `set --lock` says a `requirement` finding of the entry it wrote as a warning.
5. **Place round it**: run `netsmith build`, then `netsmith fab`. A build puts every part with an entry where the file says. `fab` says which seed placed the board (`placed on ... with seed 15`) and, when it is not the file's seed, writes the new seed into the layout file: review that diff, and commit the file with the source. `fab --locked` writes nothing and stops instead when the seed would change; use it in a release or a check, and run plain `fab` to take the new seed.
6. **One module again**: `netsmith layout regenerate <module path> [--seed N] [--margin MM]` places the module's unlocked parts again inside the box round them, every other part held, and prints what moved, the nets whose copper is routed again and the findings before and after; nothing is written without `--write`. A proposal is a proposal: keep it only when its findings and totals are better.
7. **Look**: `netsmith render <file.nsm> --view top --highlight <path> --out board.png`, and `netsmith board3d <file.nsm> --json` for numbers (see `netsmith-use-constraints`, steps 9 and 10). Quote numbers from the data, not from the picture.

## When the layout and a requirement disagree

- A locked entry that breaks a requirement is refused before placing, with the numbers. Change the entry (`layout set ... --at`), or `--unlock` it so the placer finds a place that meets the requirement, or ask the user whether the requirement is wrong. Never loosen a requirement to keep a lock.
- A locked part whose courtyard is past an edge needs the requirement that says it may hang over (`netsmith-use-constraints`, overhang); the lock does not say it.
- Two parts locked so that their courtyards overlap is a placement fault that names both. Move one.
- `no seed places the board: ... the closest is seed 7, where this blocks it:` lists what blocks the best try, by path and designator, with the amounts. Unlock or move the part it names, or give the board more room; `--place-seeds N` and `--place-budget` widen or bound the search for one run.
- Entries of parts that `netsmith routes import` wrote are `source = "import"`; they may share courtyard area and `fab` warns of each overlap.

## Report

Tell the user which parts you locked and why, which you left to the placer, each `layout check` finding left with its line, the seed `fab` wrote if it changed the file, and that comments in the layout file are dropped by `layout set`. To rearrange one module, run `netsmith layout regenerate <module>` (a proposal: what moves, the nets routed again, the findings and totals before and after), try other `--seed`s, and keep the best with `--write`; its parts get unlocked entries with `source = "regen"`. Tell the user which seed you kept and why.
