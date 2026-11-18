---
name: netsmith-prepare-fab-order
description: Check a netsmith fab package before it is ordered from JLCPCB, read fab-notes.txt, and list what a person must verify in the order preview. Use when `netsmith board fab` or `netsmith project order` has succeeded, or before anyone orders boards.
---

# Prepare a fab order

Never place an order. Prepare the package and give the user a checklist.

## What `netsmith board fab` wrote

For each board, in `fab/<Board>/`:

- `<Board>-gerbers.zip`: the Gerbers and drill files, written by netsmith itself (not KiCad; ADR 0034). `netsmith view gerber fab/<Board>` draws them as the board house will see them, a side or a layer at a time, or sums them up with `--format json`.
- `bom.csv`: bill of materials with LCSC part numbers.
- `cpl.csv`: placement: designator, position, side, rotation.
- `fab-notes.txt`: what netsmith could not verify. A run with `--offline` asked no supplier, and a line `offline: stock and price of the parts were not checked` says so: tell the user to look each part up at the supplier before ordering.
- `<Board>.kicad_pcb`, `<Board>.kicad_pro`, `<Board>.kicad_dru`, `<Board>.svg`: the board for KiCad (the `.kicad_dru` holds the process's rules that the project file cannot, such as the space between surface-mount pads, so KiCad's check agrees with netsmith's) and a picture.
- `<Board>.glb`: the board's 3D scene (glTF 2.0 binary, as routed) for a mechanical engineer or a viewer; not for the order form and not in the Gerber zip. It is absent when the run had `--no-glb` or `netsmith.toml` says `glb = false`; a `warning:` in `fab-notes.txt` says when it could not be written. Heights in it are `null` where unknown, never guessed.
- `<Board>-schematic.pdf`: the schematic as the editor draws it, a page for each sheet with a title block, for a reviewer or an assembler; not for the order form and not in the Gerber zip. It is absent when the run had `--no-pdf` or `netsmith.toml` says `pdf = false`; a `warning:` in `fab-notes.txt` says when it could not be written.
- `<Board>.step`: the board and its parts as STEP (AP214) for a mechanical engineer's CAD, with no copper; not for the order form and not in the Gerber zip. It is absent when the run had `--no-step` or `netsmith.toml` says `step = false`; a `warning:` in `fab-notes.txt` says when it could not be written. A part of unknown height is its outline only there, never a guessed solid.
- `<Board>.pins.h` and `<Board>.pins.json`, only when the board has pin requests: the pin map for the firmware author, from the pins the board was routed with (not for the order form). If `fab-notes.txt` says the pin map changed by this run, or the board is INCOMPLETE, tell the user the firmware's pins are not final and the firmware author must take the files of the package that is ordered.

For a system, each board has its own directory under `fab/<System>/`, and each is ordered on its own.

## When the design rules check stops `fab`

An error of the check (a clearance, a short, a hole, the edge, a courtyard) stops `board fab` and **no package is written**: only the KiCad board, its project, the SVG and the other files that are not for the board house are in the folder. The first line of the error says so (`design rules check: N errors: no package written (the KiCad board is in <folder>); ...`) and what each flag does. There are two ways on, and they are not the same:

- `--allow-drc-violations` packages the board **to order from**: `fab-notes.txt` says `design rules check: NOT clean, packaged anyway` and lists the errors. It is never yours to use for an order: ask the user, and name the errors you would waive.
- `--for-review` packages it **to look at**: the KiCad board and project, `<Board>-gerbers-REVIEW.zip`, the SVG, the `.glb`, `.step`, the schematic PDF, the pictures of the copper (`<Board>-top-REVIEW.png`, `<Board>-inner<n>-REVIEW.png` for each inner signal layer, `<Board>-bottom-REVIEW.png`) and `fab-notes.txt` with the findings, each marked `FOR REVIEW, NOT FOR ORDER`, and **no** `bom.csv` and **no** `cpl.csv`. The run still exits 1 while a finding it waived stands, and `project order` refuses the folder (it makes the board again, and the finding stops it). Use it when a person must look at a board with a known finding (one the board house accepts, a clearance to fix later, a part over a height limit while the case is still being drawn). Never order from it; never give its zip to the board house. It waives, besides the errors of the check, a requirement on where a part goes that is not met (`--allow-drc-violations` still never waives one) and the board's own limits that stop `fab` at placement: a part over a height limit or a keepout's `.max_height` (`too_tall`), a part or a pad in a keepout, a part over pins that protrude, a collision with a case model, and with strict constraints a part of unknown height under a limit. Overlapping courtyards, a part outside the outline or over a cutout, and lines proved unable to hold together still stop it. It takes neither `--allow-drc-violations` nor `--cross-check`.
- A finding of a part's own footprint (`(within U1's own footprint)`) that the user accepts for good is waived in the layout file, one finding at a time with a reason and a source (`netsmith-read-errors`), never by `--allow-drc-violations`. `fab-notes.txt` lists each under `design rules waived:`: read every one against its source before an order, and tell the user they are there.

## Reading a package made for review

`fab-notes.txt` begins, after its title, with `FOR REVIEW, NOT FOR ORDER: `--for-review` waived N findings, each listed below, ...`, which names the pictures, and then every finding waived, one to a line (`error [too_tall] ...`, `error [clearance] ...`, `error [constraint] ...`), each with the part, the numbers and the place. Go through every line with the person. `--format json` gives each as a finding of code `fab` with `waived: "for-review"` and its `rule`, a height's `placement` (`height`, `limit`, `keepout`) and a design rule's `place`. The layout file's waivers of a part's own footprint are not among them: they are not errors, and a package for review lists them under `design rules waived:` further down, as a package to order from does (`severity: "waived"` in `--format json`).

Look at each picture (`Read` the PNG): it is the board `fab` just routed, as `netsmith view pcb --layer <layer> --drc --islands` draws it, the check's findings as numbered rings (red an error, amber a warning) and the copper islands the fill took out hatched as `I1`, `I2`, .... The bottom is seen from below, mirrored. To light a part or a net, or to draw a region larger, run `view pcb` yourself (`netsmith-inspect-a-board`); it draws the same copper only after `board fab --for-review --reuse-routes`. `--no-pictures` leaves the pictures out. A package that is not for review has no pictures, and a later plain `fab` removes the review's zip and pictures.

## An order: several boards, several copies

If the user orders some of the project's boards, or more than one copy, use a named order (ADR 0035). `netsmith project order` lists the orders of `netsmith.toml`. To add one, write

```toml
[order.devkit.boards]
"DevKit/carrier" = 2
Unit = 10
```

(a board as its folder is named, `Board` or `System/member`, or a system's name for all its members; the quantity is the number of boards to order). Ask the user which boards and how many: do not guess quantities. `netsmith project order devkit` makes only those boards into `fab/orders/devkit/<board>/` and writes two more files:

- `order.txt`: for each board the copies, layers, size, thickness, hand-solder parts, then the combined bill of materials, each board's `fab-notes.txt` lines under "before you order", and `unverified items`. Read it to the user.
- `order-bom.csv`: one row for each LCSC number, with the quantity of the whole order (copies times parts on one board). It has **no spares**: ask the user how many extra of each part they want, and remember that JLCPCB's assembly has minimum quantities.

An order copies the package of a board that a plain `netsmith board fab` made from the same inputs and that nobody has edited, instead of making it again; the last line of `project order` says `reused N of M boards from a plain fab; K stages run`, and the progress says why a board was made again. Either way the package is the one the order would have made. A package you edited by hand is never reused: tell the user that a hand edit is lost to the order, and make the change in the source.

The surface finish is in `order.txt` (`finish HASL lead-free`, the process's `.finish`): the board house's order form has to be set to it, so read it to the user; a board states another with `.process.finish &= "ENIG"`. A panel (one board repeated in a V-cut grid, ADR 0040) is written in the source as `panel P { .cell: Board  .fab: VCutPanel { .columns &= 3  .rows &= 2  .rail &= 5mm } }` (`import std::mfg::jlcpcb::VCutPanel`); ask the user for the grid and whether it is assembled (an assembled panel needs rails, `.rail &= 5mm`). `netsmith board fab` then writes `fab/<Panel>/` with one Gerber set for the whole panel, and an order may name the panel (`P = 3` is three panels). Read the panel's `fab-notes.txt` to the user: it says what to tell the board house (a panel made by the customer, V-cut, standard assembly only), and that the designators of the panel's `bom.csv` and `cpl.csv` (`R1_3`) are **unverified**: JLCPCB publishes no rule for them and has not accepted a netsmith panel, so the single board's files in `fab/<Panel>/cell/<Board>/` are the fallback. Other panels (different boards, routed tabs, mouse bites) are not built; do not invent them.

## Read `fab-notes.txt`

| Line | What to do |
|---|---|
| `process:` | The factory rules used. Confirm the layer count and thickness are what the user wants. |
| `fictional fabricator:` | The board is on ACME (`std::mfg::acme`), which does not exist. Stop: the package cannot be ordered, and `project order` refuses it. Tell the user; to order, the board needs a real fabricator's process. |
| `hand-solder:` | These parts are not in the BOM. Tell the user to buy and solder them. |
| `consigned:` | Parts the user buys and ships to the board house (ADR 0084): they are in `bom.csv` (manufacturer's number as the comment, LCSC cell empty) and in `cpl.csv`, but JLCPCB's BOM has no column that marks them. Tell the user to register the parts on JLCPCB's site, ship the quantity listed with the order, and check the component matching step of the order preview before paying. |
| `trim:` | The design states that the pins of a through-hole part protrude only so far out of the far side of the board (`.pin_protrusion`), so a person cuts them after soldering and before the covering part is fitted. It is a hand step and nothing in the files carries it: tell the user to cut the pins as the line says, then measure one protruding pin against the line's length before the covering part goes on. It is also listed in `order.txt` under "before you order". |
| `the board house does not assemble these (hand-solder, and no LCSC part number ...)` | Parts whose catalog row has no supplier number, marked `hand`: they are not in `bom.csv`, so the user must buy them. |
| `environment:` and `environment not checked:` | Only for a board that states an environment (ADR 0099): how many fitted parts met it, were not met (zero in a written package) and were unjudged (a part whose row states no temperature range, one with no catalog row), and the fixed list of what is not checked (radiation, outgassing, screening, lot traceability, the part's own heating). Tell the user the unjudged count and name the parts, which are in the `environment guideline` warnings above it; never say the board was "qualified" for the environment. |
| `not fitted` | These parts (holes, fiducials, programming pads) are in neither the BOM nor the placement file. Nothing to buy. |
| `rotation unverified:` | A part's placement rotation has not been checked against an assembled board. Look at it in the order preview. |
| `N vias in pads:` | `N` vias stand in surface-mount pads of the parts the line names. Choose the board house's epoxy filled and capped (plated over) vias on the order form, which costs extra; the Gerbers do not say it. `project order` stops on such a board until the cost is accepted: ask the user, and only on a yes add `via_in_pad = "accepted"` to the order's `[order.<name>]` table (or run `project order <name> --accept-via-in-pad` once). If the user does not want the cost, put `.process.via_in_pad &= 0mm` in the board and fab again: some pads may then be unroutable, and the `vias in pads forbidden:` line says how many connections that cost. |
| `track impedance:` | Only for a board with inner layers: for each layer a track may be on, the width of a 50 ohm track and the impedance of the router's width, from the build-up, with the solder mask JLCPCB's calculator puts over an outer track. A design that states an impedance (`TrackImpedance`, `PairImpedance`) gets `track impedance target:` lines too, with the width that gives it on each layer and whether the width the router draws is within JLCPCB's test tolerance of 10 %. The board is not made to an impedance. If the user needs one (RF, USB, Ethernet), tell them the figures are approximate (an inner track between two different dielectric layers, a pair, a mask), to check them with JLCPCB's impedance calculator, and to choose impedance control on the order form; a line that says a layer is `not a plane` holds only where copper of a plane or pour is beside the track. |
| `bottom side unverified` | Parts on the back: their rotation and mirroring in the placement file are unchecked. Look at them in the order preview. |
| `turn off the right angles unverified` | Parts on the top turned by other than 0, 90, 180 or 270 degrees: JLCPCB's correction for such a turn is not measured, and the placement file has KiCad's turn plus the footprint's offset. Look at them in the order preview. |
| `routed long:` and `most vias:` | Nets the routing made more than twice the straight length between their pads (and 2 mm over it), and the nets with the most vias, with each length, the ratio and the vias (`--format json`: each board's `routes`). A note, not a fault. Tell the user which of them must be short (a crystal's, a clock's, a boot capacitor's, a switching node's): such a net needs a better placement or route (`netsmith-route-a-board`), and a datasheet number goes in the part's `layout` block as `route NAME .pin length LENGTH`, which `fab` then checks as a `layout guideline` warning that names the net. |
| `origin:` | The placement origin. Nothing to do unless the user needs a particular one. |
| `design rules check: passed` | netsmith's own check found no error. If the line is missing or says `NOT clean` (or `FOR REVIEW, NOT FOR ORDER`), stop. |
| `design rules warnings: N` | Findings that do not stop a package but should be read: silkscreen over an opening in the mask (`silk_over_copper`: the fabricator clips a line there and removes a text within 0.15 mm of a pad), silkscreen too near other silkscreen (`silk_overlap`) or the board edge (`silk_edge_clearance`), a solder mask web narrower than the process makes, a via in a pad the process cannot fill. Each is listed with its place. |
| `zone fill:` | netsmith filled the planes and pours itself. Look at every inner layer and pour in the order preview: clearances round holes and pads, thermal spokes on the plane's own pads. |
| `text ... has ... the built-in stroke font does not have` | Silkscreen text with a character netsmith's font lacks was drawn as a box. Change the text or tell the user. |

## Before ordering

1. Open `<Board>.svg` and `view` the schematic. Check connectors and orientation by eye.
2. Check every BOM line: value, package, LCSC number. Price and stock come from the project's supply provider (`[supply]` in `netsmith.toml`) and are `unknown` where it has none, so check stock on the order page; `netsmith catalog age` says how old they are and which are unknown. An unresolved generic part (a resistor, a ceramic capacitor, an inductor or an LED) stops `fab` with the nearest rows and whether the design's checks pass with each: pick one that passes in the design (`netsmith board swap R1 --apply C25094` writes it as the editor's Swap does). `netsmith catalog refresh --dry-run` does not update them: it only checks each row's manufacturer's number and package against LCSC's record and notes when LCSC lists stock 0, which means look at JLCPCB's page (it uses the network, so only when the user wants it).
3. JLCPCB normally charges a loading fee for each unique Extended part, unless it flags the part Preferred. Count the Extended parts in the build report and confirm the fee on the order page.
4. In JLCPCB's order preview, check the flagged rotations and that each part sits on its pads.
5. Run JLCPCB's own design check. netsmith holds to the published limits but cannot run it. Pass its findings to the user.
6. Boards with planes or pours: netsmith filled them for the Gerbers (JLCPCB has not yet been given these files in an order, so look at them first in the order preview). Look at the files themselves with `netsmith view gerber fab/<Board> --layer top` (and `bottom`, `inner1`, `all`): a PNG of what the board house gets, read back from the zip; open it and check the planes, the mask openings and the silkscreen. `--format json` lists the layers found, the holes by size and any file the reader could not read. Run `netsmith board fab --cross-check`: it compares each copper layer of the zip with the board model with netsmith's own viewer, with no KiCad, and says in `fab-notes.txt` `cross-check against netsmith's viewer: the Gerbers agree with the board model`, or what differs; when KiCad is installed it plots the board with KiCad too and says `the files agree`, or what differs (without KiCad the notes say `ran netsmith's viewer comparison alone`). The `.kicad_pcb` holds the same fill, so it opens poured in KiCad as the Gerbers have it; pressing `B` there refills it with KiCad's own filler, to compare. A warning `the zone of <net> on <layer> is written unfilled in the .kicad_pcb` says that zone opens unfilled in KiCad (press `B`); its Gerbers still have the fill.
7. Remind the user that capacitor values assume 25 C, with no temperature or ageing model.

## Report

For an order, give `order.txt`'s totals first (designs, boards, unverified items). List the board name, the layer count, the part count, each hand-soldered part, each unverified rotation, and anything the order preview or the JLCPCB check raised.
