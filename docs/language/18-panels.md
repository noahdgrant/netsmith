## Panels

A panel repeats one board in a grid on one fabrication panel, cut apart by V-cuts (ADR 0040). The board stays a stand-alone `board`: the panel only says how many copies there are and in what grid, and the board house's rules for such a panel come from a standard-library interface, as a board's process does. A panel names no nets and joins nothing.

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::{TwoLayer, VCutPanel}

board Tile {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        // Copper at least 0.4 mm from the edge, as a V-cut needs.
        .min_edge_clearance &= 0.4mm..1m
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .led: Led {
        .current &= 5mA..15mA
    }
    .r1: Resistor {}
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}

panel TilePanel {
    .cell: Tile
    .fab: VCutPanel {
        .columns &= 3
        .rows &= 2
        .rail &= 5mm
    }
}
```

The board's outline is in the board's physical file, `main.physical.nsm` here. A V-cut runs straight across the panel, so the outline is a rectangle with square corners. The panel reads the outline of its cell from the board with its physical file, so the file is found before the panel is checked.

```nsm-physical
import std::pcb::Outline

physical for Tile {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 30mm
            .corner_radius &= 0mm
        }
    }
}
```

Rules.

- `.cell: Board` names the board that is repeated. It is a `board` defined in the same file and placed in no system; a board a system places is made as the system's, not alone.
- `.fab: VCutPanel { ... }` is the board house's V-cut panel (`std::mfg::jlcpcb::VCutPanel`). `.columns` and `.rows` say the grid and have no default. `.rail` is the width of the handling rail round the grid, `0mm` (the default) for none; a rail carries the tooling holes and fiducials that assembly needs, and is 5 mm to 8 mm. The boards sit side by side with no space between them. Every other field is a limit of the board house with its source in `std/mfg/jlcpcb.nsm`.
- A panel holds that board and its rules and nothing else. It joins nothing, names no nets, and configures nothing inside its board.
- `netsmith fab` makes the board's own package in `fab/<Panel>/cell/<Board>/` and the panel's in `fab/<Panel>/`: one Gerber set for the whole panel (the boards' layers moved to their cells, one outline, the V-cut lines on `Edge.Cuts` marked "V-CUT", the drills merged, the tooling holes and fiducials of the rails), a bill of materials and a placement file for all the cells, `fab-notes.txt`, the panel as a `.kicad_pcb` (which KiCad can plot; `--cross-check` compares), and `panel.svg`. Designators in `bom.csv` and `cpl.csv` carry the cell (`R1_3` is `R1` of the third cell, counted along the rows from the top left).
- An order may name a panel, and its quantity then counts panels: `LedPanel = 3` in `[order.run.boards]` is three panels (see Orders).
- `fab` refuses a panel the board house would refuse, with the numbers: a panel outside 70 x 70 mm to 475 x 475 mm (250 x 250 mm if it is assembled), more than 25 lines along a side, lines closer than 2 mm, a board thinner than 0.6 mm, copper nearer the edge than 0.4 mm, an assembled panel without rails, a rail under 5 mm. Rounded corners are refused too.

Misuse: no board house rules.

```nsm fails
board Tile {
}

panel P {
    .cell: Tile
}
```

```text
error: panel P: it does not say who makes it, and a panel needs the board house's rules: add `import std::mfg::jlcpcb::VCutPanel` and `.fab: VCutPanel { .columns &= 3; .rows &= 2 }`
```

Misuse: the grid is not stated.

```nsm fails
import std::mfg::jlcpcb::VCutPanel

board Tile {
}

panel P {
    .cell: Tile
    .fab: VCutPanel {
        .rows &= 2
    }
}
```

```text
error: panel P: `fab` is a `VCutPanel` and has no value for `columns`: say how many, for example `.fab: VCutPanel { .columns &= 3 }`
```

Misuse: a panel holds boards, not parts or joins.

```nsm fails
import std::mfg::jlcpcb::VCutPanel
import std::parts::passive::Resistor
import std::sources::Rail

board Tile {
    .supply: Rail
}

panel P {
    .cell: Tile
    .fab: VCutPanel {
        .columns &= 2
        .rows &= 2
    }
    .r1: Resistor {}
    .cell.supply ~ .cell.supply
}
```

```text
error: a panel holds the board it repeats and its fabricator's panel rules, but `Resistor` is a part
   --> main.nsm:15:10
   |
15 |     .r1: Resistor {}
   |          ^^^^^^^^
error: a panel joins nothing: it repeats one board in a grid, and the boards stay apart
   --> main.nsm:16:5
   |
16 |     .cell.supply ~ .cell.supply
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```
