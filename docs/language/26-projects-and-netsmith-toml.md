## Projects and netsmith.toml

A project is a directory with a `netsmith.toml` at its root (ADR 0011). `netsmith init` makes one in the current directory. It writes `netsmith.toml`, `src/main.nsm` (a working LED board), `.gitignore` and `README.md`, and for the agents that work in the project an `AGENTS.md`, the skills in `.agents/skills/` (with `.claude/skills` linking to them) and this guide as `docs/netsmith/LANGUAGE.md` (ADR 0015). It never overwrites a file; an `AGENTS.md` that exists is left alone and `init` says so.

```toml
[project]
name = "blink"
top_level = ["src/main.nsm"]
out = "out"
fab = "fab"
kicad = 9
catalog_dirs = ["catalogs"]
```

Only `name` is required.

| Key | Meaning |
|---|---|
| `name` | The project name. |
| `top_level` | The files that hold boards or systems. `top_level = []` is a project of modules only: `build` and `fab` with no file then say `error: this project has no boards`. |
| `out` | Where `netsmith build` writes. |
| `fab` | Where `netsmith fab` writes. |
| `kicad` | The KiCad version of the files. Only 9 is accepted. |
| `catalog_dirs` | The folders whose `*.csv` files are the project's own catalogs, each named by its file name (`catalogs/acme_ldo.csv` is the catalog `acme_ldo`), in addition to the standard ones. Default `["catalogs"]`; `init` writes it out. See Catalog files. |
| `catalogs` | Optional. Single catalog files to add that are not in a `catalog_dirs` folder, each named by its catalog. See Catalog files. |
| `assembly` | Optional. Who assembles a part nothing else speaks for: `"fab"`, `"hand"`, `"none"` or `"consigned"`. See Who assembles a part. |
| `environment` | Optional. The environment every board has when it declares none: `"std::env::Environment"` or a definition of the project's own, `"src/vehicle.nsm::Automotive"`. See The environment of a board. |
| `guidelines` | Optional. What a design that goes against a guideline gets: `"warn"` (the default) reports it and builds, `"error"` refuses to build. `--strict` asks for `"error"` for one run. See Track width against current. |
| `place_seeds` | Optional. How many seeds the placer tries for a board before it gives up: the layout file's seed (0 if it states none) and the ones after it, 8 by default; 1 tries only the file's. It must be from 1 to 32 (issue #458). The seed that places the board is written to the layout file. |
| `track_margin` | Optional. How many times the process's least track a signal is drawn at where there is room: 1.5 by default, so JLCPCB's 0.1 mm least is drawn as 0.15 mm; 1 is the least, and it may be up to 4 (issue #431). The router tries the margin first, in one order of the lines and a quarter of the route time; a board it cannot finish at that width is routed again from the same placement at the least track, and `fab` says so. A net its current makes wider keeps that width. |
| `step` | Optional. Whether `fab` writes the board as STEP, `<board>.step`, into the fab package, for mechanical CAD: `true`, the default, writes it; `step = false` leaves it out, and removes the one of an earlier run. `fab --no-step` leaves it out for one run. See `docs/BOARD3D.md`, "In the fab package". |
| `glb` | Optional. Whether `fab` writes the board's 3D scene as `<board>.glb` into the fab package: `true`, the default, writes it; `glb = false` leaves it out, and removes the one of an earlier run. `fab --no-glb` leaves it out for one run. See `docs/BOARD3D.md`, "In the fab package". |

### Orders

An order says which boards go into one fabrication order and how many of each (ADR 0035). It is a table `[order.<name>.boards]` beside `[project]`: each key is a board as its output folder is named (`Board`, or `System/member`), or a system's name for all its members, and the value is the quantity.

```toml
[project]
name = "product"
top_level = ["src/devkit/devkit.nsm", "src/unit/unit.nsm"]

[order.devkit.boards]
"DevKit/carrier" = 2
"DevKit/power_card" = 1
Unit = 10

[order.spares.boards]
DevKit = 5
```

`netsmith fab --order` lists the orders. `netsmith fab --order devkit` makes only those boards, in `fab/orders/devkit/<board>/`, and writes `order.txt` (each board's layers, size, thickness, hand-solder parts and notes, the combined bill of materials, the unverified items) and `order-bom.csv` (each LCSC number with copies x parts per board, no spares). A plain `netsmith fab` still makes every board. See `docs/PROJECT-LAYOUT.md`.

- An order name is letters, digits, `-` and `_`. A quantity is a whole number of 1 or more.
- A board name that is not in the project, or a board named twice (`DevKit` and `"DevKit/carrier"` in one order), is an error with the manifest line, listing the boards that exist.
- A key may be a panel (ADR 0040), and its quantity then counts panels: `LedPanel = 3` orders three panels, so 18 boards if the panel holds six. `order.txt` says so, and `order-bom.csv` multiplies the panel's parts by the quantity. A board may be ordered loose and in a panel; the two are summed in the bill of materials.

Rules.

- The commands look for `netsmith.toml` in the current directory and then in each parent.
- With no file argument, `netsmith build`, `fab` and `editor` act on `top_level`. With a file they act on that file and still see the project's modules.
- Every other file under `src/` is a module: `src/parts/status.nsm` is `src::parts::status`.
- Paths in the manifest stay inside the project, and a catalog file is named for its catalog. An unknown key is refused with its line.
- A file name that cannot be written in an import is refused, naming the file.
- A `.csv` file in a `catalog_dirs` folder that is not named like a catalog is refused, naming the file.

Misuse.

```text
$ netsmith build        # netsmith.toml has `topl = [...]`
/path/to/project: netsmith.toml: TOML parse error at line 3, column 1
  |
3 | topl = ["src/main.nsm"]
  | ^^^^
unknown field `topl`, expected one of `name`, `top_level`, `out`, `fab`, `kicad`, `catalog_dirs`, `catalogs`
```

```text
$ netsmith fab --order devkit        # "Unitt" is not a board of the project
netsmith.toml:8: order `devkit` names `Unitt`, but the project has no such board, panel or system; the boards and panels are: DevKit/carrier, DevKit/power_card, Unit
```

### A part of your own, with no KiCad installed

A part you define needs a footprint, and `kicad.toml` says which (`Library:Name` of a KiCad library, or one of your own). netsmith needs no KiCad install to build and fab a board (ADR 0097), so a part of your own can have a footprint with none either, in two ways. The part and the board are the same in both:

```nsm-fragment
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2

part Sensor {
    pins {
        .vdd passive "VDD" "1"
        .out passive "OUT" "2"
        .gnd passive "GND" "3"
    }
}

board Probe {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .s: Sensor {
        .assembly &= AssemblyMode::Hand
    }
    .j: Header1x2
    .s.vdd ~ .j.p1
    .s.gnd ~ .j.p2
}
```

The part names no symbol in either row below, so it is drawn from its pin table, as an IC of the standard library is.

**Your own footprint file.** Draw the footprint in a `.kicad_mod` file, keep it in a `.pretty` folder of the project and list the folder as a `[[library]]`; the row names the footprint as `Library:Name`. This needs the file and nothing else:

```text
(footprint "Sensor" (layer "F.Cu")
  (attr smd)
  (pad "1" smd rect (at -1.5 -0.95) (size 1.2 0.7) (layers "F.Cu" "F.Mask" "F.Paste"))
  (pad "2" smd rect (at -1.5 0.95) (size 1.2 0.7) (layers "F.Cu" "F.Mask" "F.Paste"))
  (pad "3" smd rect (at 1.5 0) (size 1.2 0.7) (layers "F.Cu" "F.Mask" "F.Paste"))
  (fp_rect (start -2.5 -1.6) (end 2.5 1.6) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd"))
)
```

```toml
[[library]]
name = "own"
footprints = "libs/own.pretty"

[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "own:Sensor"
```

The file is `libs/own.pretty/Sensor.kicad_mod`. `build` and `fab` write the library name into the `fp-lib-table` beside the board, so KiCad finds the footprint.

**One part with a footprint of its own.** A `[[part]]` row maps every part of a kind in a package. A `[[member]]` row gives one part, named by its member path on its board (as the layout file names it), another footprint, in either way above. A port uses it to draw a part with the original's land pattern where that is not the standard row's: a project may not map a kind and package that `std/kicad.toml` maps, and an original may draw two parts of one kind and package with two land patterns. The symbol and the pin names stay those of the part's row, and the footprint is checked against the pin table as any other is. Here the sensor `.s` is drawn with the generator's SOT-23 instead of `own:Sensor`:

```toml
[[member]]
path = "s"
footprint = "gen:SOT23(pins=3)"
```

The board file, the bill of materials and the placement file name the member's footprint. A path given twice is refused. The rows are the project's, and its files may hold several boards, so a row whose path is no part of the board being built is not used there: check the path when you rename a member (a port's test compares every footprint with the original's, which finds a stale row).

**A footprint on the bottom face.** Draw every footprint as seen from the top, as KiCad's libraries do. A part on the bottom (`.place.side &= "bottom"`, or `side = "bottom"` in the layout file) is the footprint seen in a mirror: every x of the footprint's frame (pads, drawings, courtyard) negated, then turned by `.place.rotation`, with every layer on the other face (`F.Cu` becomes `B.Cu`, `F.CrtYd` becomes `B.CrtYd`). The board file `fab` writes stores it as KiCad's flip does, which mirrors in y: every y of the footprint's frame negated and the footprint turned by `.place.rotation` plus 180, which puts the pads, the drawings and KiCad's 3D model in the same place. A board file from KiCad stores a footprint on the back the same way, and so does a port's `placements.toml`, which is read from it: to draw a footprint from those figures, negate each y and each pad's own angle (its angle less the footprint's) in the footprint, and put each layer on the other face; the part's `.place.rotation` is the file's rotation less 180. A footprint drawn from them as they stand comes out flipped twice: its pads mirrored and on the wrong face.

**A footprint netsmith generates.** When one of netsmith's generators makes the package, the row asks for it by the generator's name and parameters, in place of `Library:Name` (ADR 0104). There is no file and no library:

```toml
[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "gen:SOT23(pins=3)"
```

The generators are `QFN`, `DFN`, `QFP`, `SO` (SOIC, SSOP, TSSOP and MSOP), `SOT23`, `PinHeader`, `PinSocket` and `BGA`. A parameter is a column of the generator's table of packages, so the pads of `gen:` are those of the table row with the same numbers: `gen:SOT23(pins=5)` has the pads of `Package_TO_SOT_SMD:SOT-23-5`. A length carries its unit (`pitch=0.5mm`), a size is one length or the least and the greatest (`lead_width=0.2-0.3mm`), a pair is `7x7mm` (across, then along) and a count pair `3x3`. The datasheet's dimensions have no default, so a QFN is:

```toml
[[part]]
kind = "Driver"
package = "QFN-48"
default = true
footprint = "gen:QFN(pins=48, pitch=0.5mm, body=7x7mm, ep=5.2x5.2mm, lead_width=0.2-0.3mm, lead_length=0.35-0.45mm)"
```

| Generator | Parameters (all required unless it says) |
|---|---|
| `QFN` | `pins` (a multiple of 4), `pitch`, `body` (`7x7mm`, or a size each: `6.9-7.1x6.9-7.1mm`), `ep` (the exposed pad), `lead_width`, `lead_length`, `paste` (optional: the paste openings over the exposed pad, `4x4`; by default no opening is wider than 1.5 mm), `heel` (optional: the heel fillet taken off, `0.05mm`) |
| `DFN` | the same, with `pins` even: leads on two edges |
| `QFP` | `pins` (a multiple of 4), `body` (one length: the body is square), `pitch`, `lead_width` (optional at 0.4, 0.5, 0.65 and 0.8 mm pitch, where MS-026 gives it), `ep` and `paste` (optional) |
| `SO` | `pins` (even), `pitch`, `body` (across the columns, then along: `3.9x4.9mm`), `span` (tip to tip of the leads of the two columns), `lead_width`, `lead_length` |
| `SOT23` | `pins` (3, 5 or 6); `span`, `lead_length`, `lead_width` and `body` are optional, and are the MO-178 outline's when left out |
| `PinHeader`, `PinSocket` | `rows` (1 or 2), `pins` (in a row, 1 to 40), `pitch` (`2.54mm` or `2mm`) |
| `BGA` | `layout` (balls across and down: `11x11`, a full grid), `pitch`, `body`, `pad` (the diameter of a ball's pad) |

The footprint is named from the generator and the parameters in alphabetical order, `gen:SOT23_pins3`, so one set of numbers is one footprint however the row spells it. A generator refuses what is not a land pattern. The message names the part, its package, the file and the line:

```toml
[[part]]
kind = "Sensor"
package = "SOT-23"
default = true
footprint = "gen:QFN(pins=16, pitch=0.5mm)"
```

```text
kicad.toml:5: part `Sensor` in package `SOT-23`: footprint `gen:QFN(pins=16, pitch=0.5mm)`: `body` is missing: the body across and along (`gen:QFN(..., body=...)`)
```

A part whose pin table has more or fewer pads than the footprint is refused when the board is built, by the check of the pin table against the footprint (see _Pads_): the message names the row of `kicad.toml` and its line, and how many pads the footprint makes. Use a `.kicad_mod` file for a package no generator makes (a connector, a crystal, a package of one maker's drawing); `kicad.toml` may mix both ways, and a part may still map to a footprint or a symbol of KiCad's libraries. That is the one thing that needs KiCad installed, and only for an id that netsmith does not make: a row that names no `symbol` is drawn from the pin table, a `gen:` footprint and every id of the standard library are netsmith's own, and a `[[library]]` of the project is read from the project. When a row names a KiCad id that none of these answers and no installed library has it, `build` and `fab` stop and say which part, which id, which line, where they looked (only if they looked) and the three ways out. When an install was found and only lacks the id (a typo, or a name that KiCad 9 does not have), the message says that KiCad is installed, where, and that it has no such id, and it does not advise installing KiCad:

```text
error: B: rf: part `Coax` in package `Jack35` needs KiCad's footprint `Connector_Audio:Jack_3.5mm_CUI_SJ1-3513N_Horizontal` (kicad.toml line 5), and no installed KiCad library has it.
footprint `Connector_Audio:Jack_3.5mm_CUI_SJ1-3513N_Horizontal` not found: no KiCad footprint libraries found (a folder with *.pretty folders); looked in:
  ...
Three ways out: install KiCad 9, so that netsmith reads its libraries; supply the file through a `[[library]]` of kicad.toml; or give that row a `gen:` footprint, for a package a generator makes (`footprint = "gen:SOT23(pins=3)"`).
```
