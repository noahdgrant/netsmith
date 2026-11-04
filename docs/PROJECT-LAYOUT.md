# Project layout: one repo, several boards

A netsmith project is a directory with a `netsmith.toml`. It can hold any number of boards and systems. This page shows how to lay out a repo that holds more than one product (for example a development kit made of several boards, and a production unit that reuses the same circuits), how to choose which boards go into a fabrication order and how many of each (orders), and what is not built (panels).

## What a project is

`netsmith init` makes the starting point:

```text
my-project/
├── netsmith.toml          the manifest: name, top-level files, out and fab directories
├── AGENTS.md              guidance for coding agents (written by init)
├── .agents/skills/        the netsmith-* skills (init writes them; .claude/skills links here)
├── docs/netsmith/LANGUAGE.md   the language guide
├── src/
│   └── main.nsm           a first board
├── out/                   generated schematics            (git-ignored)
└── fab/                   generated fabrication packages  (git-ignored)
```

The manifest's `top_level` lists the files whose boards and systems the no-argument commands build:

```toml
[project]
name = "my-project"
top_level = ["src/main.nsm"]
out = "out"
fab = "fab"
kicad = 9
```

Two more keys are optional, and orders (below) are tables beside `[project]`. `catalogs` lists the project's CSV catalog files (`catalogs/`), and `assembly = "fab"`, `"hand"` or `"none"` is the default for who assembles every part of every board that its kind, its catalog row or the board does not speak for (ADR 0028; the order is in `docs/LANGUAGE.md`, "Who assembles a part"). It is one default for the whole project, not one for each board.

## Several boards in one project

Every file in `top_level` can hold boards, a `system`, or both. `netsmith build` and `netsmith fab` with no file argument build all of them, and each board comes out in its own folder:

- a `board Name` writes `out/Name/` and `fab/Name/` (schematic, PCB, SVG, `bom.csv`, `cpl.csv`, `fab-notes.txt`, `Name-gerbers.zip`);
- a board placed in a `system Sys` as `.member: Name` writes `out/Sys/member/` and `fab/Sys/member/`, so two instances of one board design do not collide.

Naming one file builds only that file's boards: `netsmith fab src/unit/unit.nsm`. To choose boards and quantities across files, use an order (below). The editor lists every board and system of the project in its tree.

Modules are shared by importing them: `import src::modules::power::LdoStage` works from any file of the project.

A board's physical intent (its outline, cutouts, mounting holes, keepouts and where its parts must be, ADR 0048) is in a file beside its source with the same stem and the suffix `.physical.nsm`: `src/unit/unit.nsm` has `src/unit/unit.physical.nsm`, as the layout file is `unit.pcb.toml`. The file holds `physical for Board { ... }` units, is optional, and is not a module: nothing imports it.

## Example: a development kit and a production unit

A repo with a development kit (a carrier board and plug-in boards, joined as a system) and a production unit that reuses the same regulator and sensor circuits:

```text
product-repo/
├── netsmith.toml
├── AGENTS.md
├── .agents/skills/netsmith-*/        .claude/skills -> ../.agents/skills
├── docs/netsmith/LANGUAGE.md
├── catalogs/                         project catalogs (CSV rows with sources), ADR 0022
│   ├── regulator_ic.csv
│   └── sensor_ic.csv
├── kicad.toml                        KiCad mapping and project libraries, ADR 0021
├── libraries/                        the project's own symbols and footprints
│   └── acme/{acme.kicad_sym, acme.pretty/}
├── src/
│   ├── parts/                        parts that are only this project's (ICs, modules, antennas)
│   │   ├── regulator_ic.nsm
│   │   └── sensor_ic.nsm
│   ├── modules/                      circuits shared by both products
│   │   ├── power.nsm                 module PowerStage
│   │   └── sensing.nsm               module SensorBlock
│   ├── devkit/                       the development kit: one file per board, one system
│   │   ├── carrier.nsm               board Carrier
│   │   ├── power_card.nsm            board PowerCard
│   │   ├── sensor_card.nsm           board SensorCard
│   │   └── devkit.nsm                system DevKit  (mates the carrier's sockets with the cards' headers)
│   └── unit/                         the production unit
│       └── unit.nsm                  board Unit     (PowerStage + SensorBlock on one PCB)
├── out/                              generated, git-ignored
│   ├── DevKit/{carrier,power_card,sensor_card}/*.kicad_sch
│   └── Unit/*.kicad_sch
└── fab/                              generated, git-ignored
    ├── DevKit/{carrier,power_card,sensor_card}/   bom.csv, cpl.csv, gerbers zip, fab-notes.txt
    ├── Unit/
    └── orders/devkit/                 an order: order.txt, order-bom.csv and the packages it names
```

with `top_level = ["src/devkit/devkit.nsm", "src/unit/unit.nsm"]` and the order `devkit` of the section below in `netsmith.toml`.

- `netsmith fab` makes every package. `netsmith fab src/unit/unit.nsm` makes only the unit's. `netsmith fab --order devkit` makes the order below.
- Each board of the development kit is a stand-alone board (its own `board`, its own process, its own outputs). `DevKit` only says which ports meet and checks voltages and currents across the joins.
- The production unit and the development kit share `PowerStage` and `SensorBlock`; a change to a module shows up in both on the next build, and `netsmith compare` can show what changed.

## Ordering boards: orders

A project's `fab` directory holds one package for every board. To order only some of them, and several copies of each, write a named **order** in `netsmith.toml` (ADR 0035):

```toml
[order.devkit.boards]
"DevKit/carrier" = 2
"DevKit/power_card" = 1
Unit = 10
```

Each key is a board as its output folder is named: `Board`, or `System/member` for a board of a system. A system's name alone (`DevKit = 2`) means all its members, each that many times. The value is how many to order (1 or more; a board you do not order is left out). `netsmith fab --order` lists the orders, and `netsmith fab --order devkit` makes this one:

```text
fab/orders/devkit/
├── order.txt                  boards, layers, size, thickness, hand-solder parts, the combined bill of
│                              materials, "before you order" notes of each board, the unverified items
├── order-bom.csv              LCSC number, value, footprint, quantity (copies x parts, no spares), used on
├── DevKit/carrier/            the package of that board: gerbers zip, bom.csv, cpl.csv, fab-notes.txt, ...
├── DevKit/power_card/
└── Unit/
```

Only the boards named are made, and `--out <dir>` puts `orders/devkit/` under `<dir>`. An unknown board name stops the command with the manifest line and the boards that exist; an order folder from an earlier run is replaced. The combined bill of materials is the number of copies times the parts of each LCSC number on one board, with no overage: how many spares to buy is yours to decide. The editor's Build panel lists the orders and has a Make order button.

A plain `netsmith fab` still makes every board of the project, and `netsmith fab src/unit/unit.nsm` the boards of one file.

### Panels

An order is separate packages ordered together; a panel is one outline made from copies of a board (ADR 0040). A `panel Name { .cell: Board  .fab: VCutPanel { ... } }` next to the board in the same file repeats it in a V-cut grid, and a plain `netsmith fab` makes it with the boards:

```text
fab/LedPanel/                  the panel: LedPanel-gerbers.zip (one Gerber set and drill file),
│                              bom.csv and cpl.csv (R1_1 ... R1_6), fab-notes.txt, LedPanel.kicad_pcb,
│                              LedPanel.kicad_pro, panel.svg
└── cell/LedTile/              the board's own package (the single-board files)
```

An order may name the panel; its quantity counts panels:

```toml
[order.run.boards]
LedPanel = 3        # three panels: 18 boards if the panel holds six
```

`order.txt` says so and `order-bom.csv` multiplies the panel's parts by 3. Only identical boards on a V-cut grid are built (P0): the cell board is defined in the file of its panel, and mixed boards, routed tabs and mouse bites are open (issue #62).

## What is not in a repo

`out/` and `fab/` are generated: keep them out of version control. `netsmith init` writes the `.gitignore` lines.
