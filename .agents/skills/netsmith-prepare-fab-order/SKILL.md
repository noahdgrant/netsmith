---
name: netsmith-prepare-fab-order
description: Check a netsmith fab package before it is ordered from JLCPCB, read fab-notes.txt, and list what a person must verify in the order preview. Use when netsmith fab has succeeded, or before anyone orders boards.
---

# Prepare a fab order

Never place an order. Prepare the package and give the user a checklist.

## What `netsmith fab` wrote

For each board, in `fab/<Board>/`:

- `<Board>-gerbers.zip`: the Gerbers and drill files, written by netsmith itself (not KiCad; ADR 0034).
- `bom.csv`: bill of materials with LCSC part numbers.
- `cpl.csv`: placement: designator, position, side, rotation.
- `fab-notes.txt`: what netsmith could not verify.
- `<Board>.kicad_pcb`, `<Board>.kicad_pro`, `<Board>.svg`: the board for KiCad and a picture.
- `<Board>.pins.h` and `<Board>.pins.json`, only when the board has pin requests: the pin map for the firmware author, from the pins the board was routed with (not for the order form). If `fab-notes.txt` says the pin map changed by this run, or the board is INCOMPLETE, tell the user the firmware's pins are not final and the firmware author must take the files of the package that is ordered.

For a system, each board has its own directory under `fab/<System>/`, and each is ordered on its own.

## An order: several boards, several copies

If the user orders some of the project's boards, or more than one copy, use a named order (ADR 0035). `netsmith fab --order` lists the orders of `netsmith.toml`. To add one, write

```toml
[order.devkit.boards]
"DevKit/carrier" = 2
Unit = 10
```

(a board as its folder is named, `Board` or `System/member`, or a system's name for all its members; the quantity is the number of boards to order). Ask the user which boards and how many: do not guess quantities. `netsmith fab --order devkit` makes only those boards into `fab/orders/devkit/<board>/` and writes two more files:

- `order.txt`: for each board the copies, layers, size, thickness, hand-solder parts, then the combined bill of materials, each board's `fab-notes.txt` lines under "before you order", and `unverified items`. Read it to the user.
- `order-bom.csv`: one row for each LCSC number, with the quantity of the whole order (copies times parts on one board). It has **no spares**: ask the user how many extra of each part they want, and remember that JLCPCB's assembly has minimum quantities.

The surface finish is not in `order.txt` (`finish not stated`): the user chooses it on the order form. A panel (one board repeated in a V-cut grid, ADR 0040) is written in the source as `panel P { .cell: Board  .fab: VCutPanel { .columns &= 3  .rows &= 2  .rail &= 5mm } }` (`import std::mfg::jlcpcb::VCutPanel`); ask the user for the grid and whether it is assembled (an assembled panel needs rails, `.rail &= 5mm`). `netsmith fab` then writes `fab/<Panel>/` with one Gerber set for the whole panel, and an order may name the panel (`P = 3` is three panels). Read the panel's `fab-notes.txt` to the user: it says what to tell the board house (a panel made by the customer, V-cut, standard assembly only), and that the designators of the panel's `bom.csv` and `cpl.csv` (`R1_3`) are **unverified**: JLCPCB publishes no rule for them and has not accepted a netsmith panel, so the single board's files in `fab/<Panel>/cell/<Board>/` are the fallback. Other panels (different boards, routed tabs, mouse bites) are not built; do not invent them.

## Read `fab-notes.txt`

| Line | What to do |
|---|---|
| `process:` | The factory rules used. Confirm the layer count and thickness are what the user wants. |
| `hand-solder:` | These parts are not in the BOM. Tell the user to buy and solder them. |
| `consigned:` | Parts the user buys and ships to the board house (ADR 0084): they are in `bom.csv` (manufacturer's number as the comment, LCSC cell empty) and in `cpl.csv`, but JLCPCB's BOM has no column that marks them. Tell the user to register the parts on JLCPCB's site, ship the quantity listed with the order, and check the component matching step of the order preview before paying. |
| `trim:` | The design states that the pins of a through-hole part protrude only so far out of the far side of the board (`.pin_protrusion`), so a person cuts them after soldering and before the covering part is fitted. It is a hand step and nothing in the files carries it: tell the user to cut the pins as the line says, then measure one protruding pin against the line's length before the covering part goes on. It is also listed in `order.txt` under "before you order". |
| `the board house does not assemble these (hand-solder, and no LCSC part number ...)` | Parts whose catalog row has no supplier number, marked `hand`: they are not in `bom.csv`, so the user must buy them. |
| `not fitted` | These parts (holes, fiducials, programming pads) are in neither the BOM nor the placement file. Nothing to buy. |
| `rotation unverified:` | A part's placement rotation has not been checked against an assembled board. Look at it in the order preview. |
| `vias in pads:` | Some vias stand in surface-mount pads. Choose the fabricator's epoxy filled and capped (plated over) vias on the order form, which costs extra; the Gerbers do not say it. If the user does not want that, put `.process.via_in_pad &= 0mm` in the board and fab again (some pads may then be unroutable). |
| `bottom side unverified` | Parts on the back: their rotation and mirroring in the placement file are unchecked. Look at them in the order preview. |
| `origin:` | The placement origin. Nothing to do unless the user needs a particular one. |
| `design rules check: passed` | netsmith's own check found no error. If the line is missing or says `NOT clean`, stop. |
| `design rules warnings: N` | Findings that do not stop a package but should be read: silkscreen over a pad (the fabricator clips it), a solder mask web narrower than the process makes, a via in a pad the process cannot fill. Each is listed with its place. |
| `zone fill:` | netsmith filled the planes and pours itself. Look at every inner layer and pour in the order preview: clearances round holes and pads, thermal spokes on the plane's own pads. |
| `text ... has ... the built-in stroke font does not have` | Silkscreen text with a character netsmith's font lacks was drawn as a box. Change the text or tell the user. |

## Before ordering

1. Open `<Board>.svg` and `view` the schematic. Check connectors and orientation by eye.
2. Check every BOM line: value, package, LCSC number. Price and stock come from the project's supply provider (`[supply]` in `netsmith.toml`) and are `unknown` where it has none, so check stock on the order page; `netsmith catalog age` says how old they are and which are unknown. An unresolved generic part (a resistor, a ceramic capacitor, an inductor or an LED) stops `fab` with the nearest rows and whether the design's checks pass with each: pick one that passes in the design. `netsmith catalog refresh --dry-run` does not update them: it only checks each row's manufacturer's number and package against LCSC's record and notes when LCSC lists stock 0, which means look at JLCPCB's page (it uses the network, so only when the user wants it).
3. JLCPCB normally charges a loading fee for each unique Extended part, unless it flags the part Preferred. Count the Extended parts in the build report and confirm the fee on the order page.
4. In JLCPCB's order preview, check the flagged rotations and that each part sits on its pads.
5. Run JLCPCB's own design check. netsmith holds to the published limits but cannot run it. Pass its findings to the user.
6. Boards with planes or pours: netsmith filled them for the Gerbers (JLCPCB has not yet been given these files in an order, so look at them first in the order preview). Run `netsmith fab --cross-check` when KiCad is installed: it plots the board with KiCad too and says `the files agree`, or what differs. Open the `.kicad_pcb` in KiCad and press `B` to see KiCad's own fill.
7. Remind the user that capacitor values assume 25 C, with no temperature or ageing model.

## Report

For an order, give `order.txt`'s totals first (designs, boards, unverified items). List the board name, the layer count, the part count, each hand-soldered part, each unverified rotation, and anything the order preview or the JLCPCB check raised.
